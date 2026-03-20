// Async task management for axum-apcore.
//
// Provides async task submission with background execution,
// status tracking, cancellation, and cleanup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use apcore::async_task::TaskStatus;
use apcore::cancel::CancelToken;
use serde_json::Value;

use crate::config::ApcoreSettings;
use crate::errors::AxumApcoreError;

/// Manages async task submission, tracking, and cancellation.
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskEntry>>>,
    max_concurrent: usize,
    max_tasks: usize,
}

/// Serializable task info returned from list/status queries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub module_id: String,
    pub status: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
struct TaskEntry {
    pub status: TaskStatus,
    pub module_id: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub cancel_token: CancelToken,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl TaskEntry {
    fn to_info(&self, task_id: &str) -> TaskInfo {
        TaskInfo {
            task_id: task_id.to_string(),
            module_id: self.module_id.clone(),
            status: format!("{:?}", self.status),
            result: self.result.clone(),
            error: self.error.clone(),
            created_at: self.created_at.to_rfc3339(),
        }
    }
}

impl TaskManager {
    /// Create a new TaskManager from settings.
    pub fn from_settings(settings: &ApcoreSettings) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent: settings.task_max_concurrent,
            max_tasks: settings.task_max_tasks,
        }
    }

    /// Submit a new async task. Returns the task ID and a CancelToken.
    pub fn submit(
        &self,
        task_id: String,
        module_id: String,
    ) -> Result<(String, CancelToken), AxumApcoreError> {
        let mut tasks = self.tasks.lock().expect("task lock poisoned");

        if tasks.len() >= self.max_tasks {
            return Err(AxumApcoreError::Execution(apcore::ModuleError::new(
                apcore::ErrorCode::GeneralInternalError,
                format!("Maximum task limit reached ({})", self.max_tasks),
            )));
        }

        let active_count = tasks
            .values()
            .filter(|t| matches!(t.status, TaskStatus::Running))
            .count();

        if active_count >= self.max_concurrent {
            return Err(AxumApcoreError::Execution(apcore::ModuleError::new(
                apcore::ErrorCode::GeneralInternalError,
                format!(
                    "Maximum concurrent task limit reached ({})",
                    self.max_concurrent
                ),
            )));
        }

        let cancel_token = CancelToken::new();
        tasks.insert(
            task_id.clone(),
            TaskEntry {
                status: TaskStatus::Running,
                module_id,
                result: None,
                error: None,
                cancel_token: cancel_token.clone(),
                created_at: chrono::Utc::now(),
            },
        );

        Ok((task_id, cancel_token))
    }

    /// Get the status of a task.
    pub fn get_status(&self, task_id: &str) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().expect("task lock poisoned");
        tasks.get(task_id).map(|t| t.status)
    }

    /// Get full info for a task.
    pub fn get_task_info(&self, task_id: &str) -> Option<TaskInfo> {
        let tasks = self.tasks.lock().expect("task lock poisoned");
        tasks.get(task_id).map(|t| t.to_info(task_id))
    }

    /// Get the result of a completed task.
    pub fn get_result(&self, task_id: &str) -> Option<Value> {
        let tasks = self.tasks.lock().expect("task lock poisoned");
        tasks.get(task_id).and_then(|t| {
            if matches!(t.status, TaskStatus::Completed) {
                t.result.clone()
            } else {
                None
            }
        })
    }

    /// List tasks, optionally filtered by status.
    pub fn list_tasks(&self, status_filter: Option<&str>) -> Vec<TaskInfo> {
        let tasks = self.tasks.lock().expect("task lock poisoned");
        tasks
            .iter()
            .filter(|(_, entry)| {
                status_filter
                    .map(|s| format!("{:?}", entry.status).to_lowercase() == s.to_lowercase())
                    .unwrap_or(true)
            })
            .map(|(id, entry)| entry.to_info(id))
            .collect()
    }

    /// Mark a task as completed with a result.
    pub fn complete(&self, task_id: &str, result: Value) {
        let mut tasks = self.tasks.lock().expect("task lock poisoned");
        if let Some(entry) = tasks.get_mut(task_id) {
            entry.status = TaskStatus::Completed;
            entry.result = Some(result);
        }
    }

    /// Mark a task as failed with an error message.
    pub fn fail(&self, task_id: &str, error: String) {
        let mut tasks = self.tasks.lock().expect("task lock poisoned");
        if let Some(entry) = tasks.get_mut(task_id) {
            entry.status = TaskStatus::Failed;
            entry.error = Some(error);
        }
    }

    /// Cancel a running task.
    pub fn cancel(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().expect("task lock poisoned");
        if let Some(entry) = tasks.get_mut(task_id) {
            if matches!(entry.status, TaskStatus::Running) {
                entry.cancel_token.cancel();
                entry.status = TaskStatus::Cancelled;
                return true;
            }
        }
        false
    }

    /// Remove completed/failed/cancelled tasks older than the cleanup age.
    pub fn cleanup(&self, max_age_secs: u64) -> usize {
        let mut tasks = self.tasks.lock().expect("task lock poisoned");
        let before = tasks.len();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        tasks.retain(|_, entry| {
            matches!(entry.status, TaskStatus::Running) || entry.created_at > cutoff
        });
        before - tasks.len()
    }

    /// Count tasks by status.
    pub fn count(&self) -> (usize, usize, usize, usize) {
        let tasks = self.tasks.lock().expect("task lock poisoned");
        let mut running = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut cancelled = 0;
        for entry in tasks.values() {
            match entry.status {
                TaskStatus::Running => running += 1,
                TaskStatus::Completed => completed += 1,
                TaskStatus::Failed => failed += 1,
                TaskStatus::Cancelled => cancelled += 1,
                _ => {}
            }
        }
        (running, completed, failed, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> TaskManager {
        let settings = ApcoreSettings::default();
        TaskManager::from_settings(&settings)
    }

    #[test]
    fn test_submit_and_get_status() {
        let mgr = make_manager();
        let (id, _token) = mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        assert_eq!(id, "task-1");
        assert!(matches!(
            mgr.get_status("task-1"),
            Some(TaskStatus::Running)
        ));
    }

    #[test]
    fn test_complete_task() {
        let mgr = make_manager();
        mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        mgr.complete("task-1", serde_json::json!({"result": 42}));
        assert!(matches!(
            mgr.get_status("task-1"),
            Some(TaskStatus::Completed)
        ));
    }

    #[test]
    fn test_get_result() {
        let mgr = make_manager();
        mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        assert!(mgr.get_result("task-1").is_none()); // Not completed yet
        mgr.complete("task-1", serde_json::json!({"val": 1}));
        assert_eq!(
            mgr.get_result("task-1").unwrap(),
            serde_json::json!({"val": 1})
        );
    }

    #[test]
    fn test_fail_task() {
        let mgr = make_manager();
        mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        mgr.fail("task-1", "something went wrong".into());
        assert!(matches!(mgr.get_status("task-1"), Some(TaskStatus::Failed)));
    }

    #[test]
    fn test_cancel_task() {
        let mgr = make_manager();
        let (_id, token) = mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        assert!(!token.is_cancelled());
        assert!(mgr.cancel("task-1"));
        assert!(token.is_cancelled());
        assert!(matches!(
            mgr.get_status("task-1"),
            Some(TaskStatus::Cancelled)
        ));
    }

    #[test]
    fn test_cancel_completed_task_fails() {
        let mgr = make_manager();
        mgr.submit("task-1".into(), "mod.a".into()).unwrap();
        mgr.complete("task-1", serde_json::json!(null));
        assert!(!mgr.cancel("task-1"));
    }

    #[test]
    fn test_get_status_nonexistent() {
        let mgr = make_manager();
        assert!(mgr.get_status("nonexistent").is_none());
    }

    #[test]
    fn test_list_tasks() {
        let mgr = make_manager();
        mgr.submit("t1".into(), "mod.a".into()).unwrap();
        mgr.submit("t2".into(), "mod.b".into()).unwrap();
        mgr.complete("t1", serde_json::json!(null));

        let all = mgr.list_tasks(None);
        assert_eq!(all.len(), 2);

        let running = mgr.list_tasks(Some("running"));
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].task_id, "t2");
    }

    #[test]
    fn test_count() {
        let mgr = make_manager();
        mgr.submit("t1".into(), "mod.a".into()).unwrap();
        mgr.submit("t2".into(), "mod.b".into()).unwrap();
        mgr.complete("t1", serde_json::json!(null));
        let (running, completed, failed, cancelled) = mgr.count();
        assert_eq!(running, 1);
        assert_eq!(completed, 1);
        assert_eq!(failed, 0);
        assert_eq!(cancelled, 0);
    }

    #[test]
    fn test_cleanup() {
        let mgr = make_manager();
        mgr.submit("t1".into(), "mod.a".into()).unwrap();
        mgr.complete("t1", serde_json::json!(null));
        // Cleanup with 0 age = remove everything not running
        let removed = mgr.cleanup(0);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_get_task_info() {
        let mgr = make_manager();
        mgr.submit("t1".into(), "mod.a".into()).unwrap();
        let info = mgr.get_task_info("t1").unwrap();
        assert_eq!(info.task_id, "t1");
        assert_eq!(info.module_id, "mod.a");
        assert_eq!(info.status, "Running");
    }
}
