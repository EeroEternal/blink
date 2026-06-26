#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxTier {
    /// One-shot ephemeral sandbox (free tier).
    Ephemeral,
    /// Named persistent session (paid tier).
    Session,
}
