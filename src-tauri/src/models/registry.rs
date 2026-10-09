/// What a model is for: System One decisions, or chat with tools (company research).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Decision,
    Chat,
}

/// A downloadable model. Values come from the 2026-10-08 spikes.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub hf_repo: &'static str,
    pub hf_file: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub min_ram_gb: u32,
    pub kind: ModelKind,
}

pub const CLEF_FLASH: ModelSpec = ModelSpec {
    id: "clef-flash",
    label: "Clef-Flash (default)",
    hf_repo: "ggml-org/Clef-Flash-GGUF",
    hf_file: "Clef-Flash-Q4_K_M.gguf",
    sha256: "fd3e90605e8103307dca37cb5a8cdb036267e2fe3cb2d908d80a8ceb9ec0638c",
    size_bytes: 6_486_448_288,
    min_ram_gb: 8,
    kind: ModelKind::Decision,
};

pub const CLEF_27B: ModelSpec = ModelSpec {
    id: "clef-27b",
    label: "Clef 27B (optional)",
    hf_repo: "ggml-org/Clef-GGUF",
    hf_file: "Clef-Q4_K_M.gguf",
    sha256: "bd9b8ae24c5752a2bb5e2733d5168340d5f04457e02b8bf0d1bc9285ec3d4f61",
    size_bytes: 19_232_219_200,
    min_ram_gb: 20,
    kind: ModelKind::Decision,
};

pub const KEV_4B: ModelSpec = ModelSpec {
    id: "kev-4b",
    label: "Kev-4B (fallback)",
    hf_repo: "ggml-org/Kev-4B-GGUF",
    hf_file: "Kev-4B-Q4_K_M.gguf",
    sha256: "33ae6b18926502b2209a1bf7d3b61a350d65938441515c686bb87d226a19eff9",
    size_bytes: 3_033_489_824,
    min_ram_gb: 6,
    kind: ModelKind::Decision,
};

pub const D1_3B: ModelSpec = ModelSpec {
    id: "d1-3b",
    label: "Liquid d1-3B (small)",
    hf_repo: "LiquidAI/d1-3B-GGUF",
    hf_file: "d1-3B-Q4_K_M.gguf",
    sha256: "16aff27ea2eefdc32b9897f43854a5d3170c1dc8dccb9c756905af30a4e22402",
    size_bytes: 1_674_456_672,
    min_ram_gb: 4,
    kind: ModelKind::Decision,
};

pub const LFM25_2_6B: ModelSpec = ModelSpec {
    id: "lfm2.5-2.6b",
    label: "Liquid LFM2.5 2.6B (research)",
    hf_repo: "LiquidAI/LFM2.5-2.6B-GGUF",
    hf_file: "LFM2.5-2.6B-Q4_K_M.gguf",
    sha256: "02a8b7e17487d326e46d68ce0ba24211e1b80a14c4cd0597fa73c1cd697f52ed",
    size_bytes: 1_674_455_040,
    min_ram_gb: 4,
    kind: ModelKind::Chat,
};

/// Decision models keep their original indices (see `recommended`); chat models follow.
pub static REGISTRY: &[ModelSpec] = &[CLEF_FLASH, CLEF_27B, KEV_4B, D1_3B, LFM25_2_6B];
