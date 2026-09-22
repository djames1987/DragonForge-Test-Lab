#![no_main]

use df_test_protocol::JobRequest;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(job) = serde_json::from_slice::<JobRequest>(data) {
        let _ = job.required_capabilities();
        let _ = serde_json::to_vec(&job);
    }
});
