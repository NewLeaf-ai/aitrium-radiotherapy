pub mod manual_jsonrpc;

use crate::tools::ToolRegistry;

pub trait TransportAdapter {
    fn run(&self, registry: &ToolRegistry) -> anyhow::Result<()>;
}
