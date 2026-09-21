use df_test_protocol::{JobRequest, WorkerRegistration, PROTOCOL_VERSION};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

#[derive(Debug, Default)]
pub struct Controller {
    queued: VecDeque<JobRequest>,
    workers: HashMap<String, WorkerRegistration>,
}

impl Controller {
    pub fn register_worker(&mut self, worker: WorkerRegistration) -> Result<(), ControllerError> {
        if worker.protocol_version != PROTOCOL_VERSION {
            return Err(ControllerError::ProtocolMismatch);
        }
        self.workers.insert(worker.worker_id.clone(), worker);
        Ok(())
    }

    pub fn enqueue(&mut self, job: JobRequest) {
        self.queued.push_back(job);
    }

    pub fn queued_jobs(&self) -> usize {
        self.queued.len()
    }

    pub fn assign_next(&mut self, worker_id: &str) -> Result<Option<JobRequest>, ControllerError> {
        let worker = self
            .workers
            .get(worker_id)
            .ok_or(ControllerError::UnknownWorker)?;

        let position = self.queued.iter().position(|job| {
            job.required_capabilities().is_subset(&worker.capabilities)
        });

        Ok(position.and_then(|index| self.queued.remove(index)))
    }

    pub fn cancel(&mut self, job_id: Uuid) -> bool {
        if let Some(index) = self.queued.iter().position(|job| job.id == job_id) {
            self.queued.remove(index);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControllerError {
    ProtocolMismatch,
    UnknownWorker,
}

#[cfg(test)]
mod tests {
    use super::*;
    use df_test_protocol::{Capability, RepositorySpec, TestAction};

    #[test]
    fn scheduler_skips_jobs_worker_cannot_run() {
        let mut controller = Controller::default();
        controller
            .register_worker(WorkerRegistration {
                worker_id: "linux-1".into(),
                protocol_version: PROTOCOL_VERSION,
                os: "linux".into(),
                arch: "x86_64".into(),
                capabilities: [Capability::CargoTest].into_iter().collect(),
            })
            .unwrap();

        controller.enqueue(JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/a.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoBuild { release: false }],
        ));
        controller.enqueue(JobRequest::new(
            RepositorySpec {
                url: "https://github.com/example/b.git".into(),
                revision: "main".into(),
            },
            vec![TestAction::CargoTest { all_features: false }],
        ));

        let assigned = controller.assign_next("linux-1").unwrap().unwrap();
        assert!(matches!(assigned.actions[0], TestAction::CargoTest { .. }));
        assert_eq!(controller.queued_jobs(), 1);
    }
}
