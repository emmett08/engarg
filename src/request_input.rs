use clap::Args;
use engarg::{
    Error, bounded_file,
    domain::{Evidence, Request, validate_request},
};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf};

pub(crate) const DEFAULT_MAX_INPUT_BYTES: usize = 1_048_576;

#[derive(Args)]
#[group(skip)]
pub(crate) struct Source {
    /// Plain-English claim to develop or check.
    #[arg(long, required_unless_present = "input", conflicts_with = "input")]
    claim: Option<String>,
    /// Prepared request JSON with a claim and optional context and evidence.
    #[arg(long)]
    input: Option<PathBuf>,
    /// Background and scope for the claim.
    #[arg(long, requires = "claim", conflicts_with = "input")]
    context: Option<String>,
    /// Attach a local UTF-8 file as user-supplied evidence. May be repeated.
    #[arg(long = "evidence-file", value_name = "PATH")]
    evidence_files: Vec<PathBuf>,
}

impl Source {
    pub(crate) fn load(self, limit: usize) -> Result<Request, Error> {
        if limit == 0 {
            return Err(Error::InvalidInput(
                "resource limits must be positive".into(),
            ));
        }
        let mut request = match (self.claim, self.input) {
            (Some(claim), None) => Request {
                claim,
                context: self.context.unwrap_or_default(),
                evidence: Vec::new(),
            },
            (None, Some(path)) => crate::read_json(&path, limit)?,
            _ => {
                return Err(Error::InvalidInput(
                    "supply exactly one of --claim or --input".into(),
                ));
            }
        };
        validate_request(&request).map_err(|errors| Error::InvalidInput(errors.join("; ")))?;
        let budget_error = || input_limit(limit);
        let mut remaining = limit
            .checked_sub(serde_json::to_vec(&request)?.len())
            .ok_or_else(budget_error)?;
        let used_ids: HashSet<_> = request
            .evidence
            .iter()
            .map(|item| item.id.clone())
            .collect();
        let mut numbers = (1..).map(|number| format!("file_{number}"));
        for path in self.evidence_files {
            let bytes = bounded_file::read(&path, remaining, "evidence file")?;
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let excerpt = String::from_utf8(bytes).map_err(|_| {
                Error::InvalidInput(format!(
                    "evidence file must be valid UTF-8: {}",
                    path.display()
                ))
            })?;
            let title = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| Error::InvalidInput("evidence file must have a name".into()))?;
            if excerpt.trim().is_empty() {
                return Err(Error::InvalidInput(format!(
                    "evidence file must contain text: {}",
                    path.display()
                )));
            }
            let id = numbers
                .find(|id| !used_ids.contains(id))
                .expect("bounded requests cannot exhaust evidence identifiers");
            let evidence = Evidence {
                id,
                title,
                source: format!("sha256:{digest}"),
                excerpt,
                kind: "local-file".into(),
                checked_by_user: false,
            };
            let separator = usize::from(!request.evidence.is_empty());
            remaining = remaining
                .checked_sub(serde_json::to_vec(&evidence)?.len() + separator)
                .ok_or_else(budget_error)?;
            request.evidence.push(evidence);
        }
        Ok(request)
    }
}

#[derive(Args)]
pub(crate) struct PrepareRequest {
    #[command(flatten)]
    source: Source,
    /// Maximum number of bytes in the prepared request JSON.
    #[arg(long, default_value_t = DEFAULT_MAX_INPUT_BYTES)]
    max_input_bytes: usize,
    /// New output file. Omit to write JSON to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

impl PrepareRequest {
    pub(crate) fn run(self) -> Result<(), Error> {
        let request = self.source.load(self.max_input_bytes)?;
        let content = serde_json::to_string(&request)?;
        if content.len() >= self.max_input_bytes {
            return Err(input_limit(self.max_input_bytes));
        }
        crate::emit(self.output.as_deref(), content)
    }
}

fn input_limit(limit: usize) -> Error {
    Error::Budget(format!("request exceeds the {limit}-byte input limit"))
}
