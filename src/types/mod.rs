// The Anthropic wire format itself lives in the `clewdr-anthropic` crate, so
// that the line between "what the protocol says" and "what clewdr does with
// it" is one the compiler enforces. Import it as `anthropic_wire::…`.
pub mod claude_web;
pub mod model;
pub mod oai;
