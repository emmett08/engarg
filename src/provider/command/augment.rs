//! Stable Auggie 0.x final-output print-mode requests and text admission.
use super::{Completion, CompletionRequest, Invocation, native};
use crate::Error;

pub(super) fn encode(
    argv: &[String],
    model: &str,
    request: &CompletionRequest,
    max_output_tokens: u32,
) -> Result<Invocation, Error> {
    let mut argv = argv.to_vec();
    argv.extend(["--print", "--quiet", "--model", model, native::INSTRUCTION].map(str::to_owned));
    Ok(Invocation {
        argv,
        input: native::prompt(request, max_output_tokens)?,
    })
}

pub(super) fn decode(bytes: &[u8], model: &str) -> Result<Completion, Error> {
    let content = std::str::from_utf8(bytes).map_err(|_| {
        Error::Provider("Augment returned non-UTF-8 completion text; no automatic retry".into())
    })?;
    Ok(native::completion(
        content.to_owned(),
        "augment",
        model,
        None,
    ))
}
