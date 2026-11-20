/// One bus and its named paints, as the dedicated server offers them. The catalogue
/// is informative: secure mod transfer still requires an authenticated manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FleetPaints {
    pub bus: String,
    pub paints: Vec<String>,
    /// Whole model, including every advertised paint. Informational only: clients
    /// may compare local bytes, but may not download based on this status alone.
    #[serde(default)]
    pub content: String,
    /// The model can be driven when installed locally, but its files cannot be downloaded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub local_only: bool,
}
