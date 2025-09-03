use crate::probe_result::ProbeResult;
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ProbeReporter: Send + Sync {
    async fn report(&self, results: &[ProbeResult]) -> Result<()>;

    fn is_continuous(&self) -> bool {
        true
    }

    fn name(&self) -> &str;
}
