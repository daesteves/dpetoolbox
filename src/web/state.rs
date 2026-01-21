use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Job status
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// A background job
#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: String,
    pub job_type: String,
    pub status: JobStatus,
    pub progress: u32,
    pub message: String,
    pub output: Vec<String>,
    pub created_at: String,
}

impl Job {
    pub fn new(job_type: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            job_type: job_type.to_string(),
            status: JobStatus::Pending,
            progress: 0,
            message: "Pending...".to_string(),
            output: Vec::new(),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }
}

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub jobs: Arc<RwLock<HashMap<String, Job>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn create_job(&self, job_type: &str) -> Job {
        let job = Job::new(job_type);
        self.jobs.write().insert(job.id.clone(), job.clone());
        job
    }

    pub fn update_job<F>(&self, job_id: &str, updater: F)
    where
        F: FnOnce(&mut Job),
    {
        if let Some(job) = self.jobs.write().get_mut(job_id) {
            updater(job);
        }
    }

    pub fn get_job(&self, job_id: &str) -> Option<Job> {
        self.jobs.read().get(job_id).cloned()
    }

    pub fn get_all_jobs(&self) -> Vec<Job> {
        self.jobs.read().values().cloned().collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
