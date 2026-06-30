// Axum context factory — extract apcore Context from Axum requests.
//
// Provides an Axum extractor (`ApContext`) and a factory (`AxumContextFactory`)
// that maps Axum request state to apcore Identity and Context.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use apcore::context::{Context, Identity};
use apcore::trace_context::TraceParent;

use crate::errors::AxumApcoreError;

/// Identity information stored in Axum request extensions.
///
/// Middleware (e.g., JWT auth) should insert this into request extensions
/// before handlers run:
///
/// ```ignore
/// use axum_apcore::RequestIdentity;
/// req.extensions_mut().insert(RequestIdentity {
///     id: "user-123".into(),
///     identity_type: "user".into(),
///     roles: vec!["admin".into()],
///     attrs: Default::default(),
/// });
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestIdentity {
    pub id: String,
    #[serde(default = "default_identity_type")]
    pub identity_type: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub attrs: HashMap<String, serde_json::Value>,
}

fn default_identity_type() -> String {
    "user".to_string()
}

impl From<RequestIdentity> for Identity {
    fn from(ri: RequestIdentity) -> Self {
        // apcore 0.16 made Identity fields private; construct via the
        // canonical `Identity::new` constructor.
        Identity::new(ri.id, ri.identity_type, ri.roles, ri.attrs)
    }
}

/// Axum extractor that produces an apcore `Context<serde_json::Value>`.
///
/// Extracts identity from request extensions (`RequestIdentity`) and
/// W3C TraceContext from the `traceparent` header.
///
/// # Usage
///
/// ```ignore
/// async fn handler(
///     ApContext(ctx): ApContext,
///     Json(input): Json<Value>,
/// ) -> Result<Json<Value>, AxumApcoreError> {
///     // ctx is a fully populated apcore Context
///     Ok(Json(serde_json::json!({"trace_id": ctx.trace_id})))
/// }
/// ```
pub struct ApContext(pub Context<serde_json::Value>);

impl<S> FromRequestParts<S> for ApContext
where
    S: Send + Sync,
{
    type Rejection = AxumApcoreError;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let factory = AxumContextFactory;
        let result = factory.create_from_parts(parts);
        async move { result.map(ApContext) }
    }
}

/// Factory for creating apcore contexts from Axum request parts.
pub struct AxumContextFactory;

impl AxumContextFactory {
    /// Create an apcore Context from Axum request parts.
    ///
    /// Uses the apcore `Context::builder` so the inbound W3C `traceparent`
    /// header propagates correctly: apcore 0.16 removed the `trace_context`
    /// field and 0.22 routes trace propagation through `TraceParent`, deriving
    /// the context `trace_id` (and stashing trace-flags/tracestate in the data
    /// map) at build time.
    pub fn create_from_parts(
        &self,
        parts: &Parts,
    ) -> Result<Context<serde_json::Value>, AxumApcoreError> {
        let identity = self.extract_identity(parts);
        let trace_parent = self.extract_trace_parent(parts);

        let ctx = Context::builder()
            .identity(Some(identity))
            .trace_parent(trace_parent)
            .build();

        Ok(ctx)
    }

    /// Extract identity from request extensions, with fallback to anonymous.
    fn extract_identity(&self, parts: &Parts) -> Identity {
        if let Some(ri) = parts.extensions.get::<RequestIdentity>() {
            ri.clone().into()
        } else {
            Identity::new(
                "anonymous".into(),
                "anonymous".into(),
                vec![],
                HashMap::new(),
            )
        }
    }

    /// Extract a W3C `TraceParent` from the `traceparent` header.
    fn extract_trace_parent(&self, parts: &Parts) -> Option<TraceParent> {
        let header = parts.headers.get("traceparent")?.to_str().ok()?;
        TraceParent::parse(header).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn test_request_identity_into_identity() {
        let ri = RequestIdentity {
            id: "user-1".into(),
            identity_type: "user".into(),
            roles: vec!["admin".into()],
            attrs: HashMap::new(),
        };
        let identity: Identity = ri.into();
        assert_eq!(identity.id(), "user-1");
        assert_eq!(identity.identity_type(), "user");
        assert_eq!(identity.roles(), ["admin"]);
    }

    #[test]
    fn test_extract_identity_anonymous_fallback() {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();
        let factory = AxumContextFactory;
        let identity = factory.extract_identity(&parts);
        assert_eq!(identity.id(), "anonymous");
        assert_eq!(identity.identity_type(), "anonymous");
    }

    #[test]
    fn test_extract_identity_from_extensions() {
        let mut req = Request::builder().body(()).unwrap();
        req.extensions_mut().insert(RequestIdentity {
            id: "user-42".into(),
            identity_type: "service".into(),
            roles: vec!["reader".into()],
            attrs: HashMap::new(),
        });
        let (parts, _) = req.into_parts();
        let factory = AxumContextFactory;
        let identity = factory.extract_identity(&parts);
        assert_eq!(identity.id(), "user-42");
        assert_eq!(identity.identity_type(), "service");
    }

    #[test]
    fn test_create_from_parts() {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();
        let factory = AxumContextFactory;
        let ctx = factory.create_from_parts(&parts).unwrap();
        assert_eq!(ctx.identity.as_ref().unwrap().id(), "anonymous");
        assert!(!ctx.trace_id.is_empty());
    }

    #[test]
    fn test_extract_trace_parent_none() {
        let req = Request::builder().body(()).unwrap();
        let (parts, _) = req.into_parts();
        let factory = AxumContextFactory;
        assert!(factory.extract_trace_parent(&parts).is_none());
    }

    #[test]
    fn test_create_from_parts_propagates_traceparent() {
        // A valid inbound W3C traceparent must seed the context trace_id with
        // the same 32-hex trace-id portion (apcore accept-or-regenerate).
        let trace_id = "0af7651916cd43dd8448eb211c80319c";
        let header = format!("00-{trace_id}-b7ad6b7169203331-01");
        let req = Request::builder()
            .header("traceparent", header)
            .body(())
            .unwrap();
        let (parts, _) = req.into_parts();
        let factory = AxumContextFactory;
        let ctx = factory.create_from_parts(&parts).unwrap();
        assert_eq!(ctx.trace_id, trace_id);
    }
}
