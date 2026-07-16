# axum-apcore ACL demo

Shows how an Axum application enforces apcore **Access Control Lists (ACL)** on
apcore module calls made from route handlers — the same `orders.delete`
(admins only) / `orders.list` (public read) contract used across the apcore
framework integrations.

## What it shows

| Call | Roles | Result |
| --- | --- | --- |
| `DELETE /orders/1` | *(none — anonymous)* | **403** |
| `DELETE /orders/1` | `user` | **403** |
| `DELETE /orders/1` | `admin` | **200** `{"deleted": 1}` |
| `GET /orders` | *(any)* | **200** (read is public) |

## How it works

1. `APCORE_ACL_PATH` points apcore at [`acl.yaml`](./acl.yaml); the singleton
   Executor is built with that ACL and enforces it on every module call.
2. `build_apcore()` registers `orders.delete` / `orders.list` as apcore modules
   with explicit IDs (via `ScannedModule` + `register_handler`).
3. The `inject_identity` middleware turns a comma-separated `X-Roles` header into
   a `RequestIdentity` request extension; the `ApContext` extractor reads it into
   an apcore `Identity(roles=...)`.
4. Each handler calls the module through `AxumApcore::call(id, inputs, Some(&ctx))`.
   A denied call returns an `ACLDenied` error, which `AxumApcoreError`'s
   `IntoResponse` maps to **HTTP 403**.

`acl.yaml` (first-match-wins, `default_effect: deny`):

- **admins** (`roles: [admin]`) may call any module;
- **anyone** (including anonymous) may call `orders.list`;
- everything else falls through to `deny`.

## Run it

```bash
APCORE_ACL_PATH=examples/acl_demo/acl.yaml cargo run --example acl_demo

curl -X DELETE localhost:3000/orders/1                     # 403 (anonymous)
curl -X DELETE localhost:3000/orders/1 -H 'X-Roles: user'  # 403 (not admin)
curl -X DELETE localhost:3000/orders/1 -H 'X-Roles: admin' # 200
curl localhost:3000/orders                                 # 200 (public read)
```

> **NOTE:** The `X-Roles` header is a demo shortcut standing in for real
> authentication. In production, populate `RequestIdentity` from a JWT/session
> middleware instead.
