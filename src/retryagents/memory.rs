use crate::{
    config::MemoryRetryAgentConfig,
    hub::{Mail, MailAgent, MailRetryAgent, RetryAgentMessage},
};
use log::{info, warn};
use std::{
    collections::VecDeque,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime},
};

pub struct MemoryRetryAgent {
    log_target: String,
    config: MemoryRetryAgentConfig,
    worker: Option<thread::JoinHandle<()>>,
}
impl MemoryRetryAgent {
    pub fn new(config: &MemoryRetryAgentConfig) -> Self {
        Self {
            log_target: "RetryAgent[Memory]".to_string(),
            config: config.clone(),
            worker: None,
        }
    }
}
impl MailAgent for MemoryRetryAgent {
    fn join(&mut self) {
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("Thread exited with errors");
    }
}
impl MailRetryAgent for MemoryRetryAgent {
    fn start(&mut self, channel: crate::hub::HubRetryAgentChannel) {
        let config = self.config.clone();
        let log_target = self.log_target.clone();

        self.worker = Some(thread::spawn(move || {
            let mut queue: VecDeque<(SystemTime, String, Mail)> = VecDeque::new();

            loop {
                match channel.next_timeout(Duration::from_secs(1)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        // shutdown
                        if !queue.is_empty() {
                            warn!(
                                target: &log_target,
                                "There were {} mails queued for retry. These are permanently lost.",
                                queue.len()
                            );
                        }
                        break;
                    }
                    Ok(RetryAgentMessage::QueueMail { dstname, mail }) => {
                        let retransmission_timepoint =
                            SystemTime::now() + Duration::from_secs(config.delay);
                        info!(
                            target: &log_target,
                            "Queueing mail for retransmission in {}s", config.delay
                        );
                        queue.push_back((retransmission_timepoint, dstname, mail));
                    }
                }

                // see if any of the queued mails is due
                let now = SystemTime::now();
                for i in 0..queue.len() {
                    if queue.get(i).unwrap().0 < now {
                        info!(
                            target: &log_target,
                            "Mail due for retransmission. Queueing."
                        );
                        let mail = queue.pop_front().unwrap();
                        channel.notify_retry_mail(mail.1, mail.2)
                    } else {
                        // The mails are stored in the order in which they were queued.
                        // If the first isn't due, neither is every mail behind that.
                        break;
                    }
                }
            }
            info!(target: &log_target, "Stopping");
        }));
    }
}
