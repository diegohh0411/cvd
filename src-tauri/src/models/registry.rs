/// A downloadable decision model. Values come from the 2026-10-08 spike.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub hf_repo: &'static str,
    pub hf_file: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub min_ram_gb: u32,
}

pub const CLEF_FLASH: ModelSpec = ModelSpec {
    id: "clef-flash",
    label: "Clef-Flash (default)",
    hf_repo: "ggml-org/Clef-Flash-GGUF",
    hf_file: "Clef-Flash-Q4_K_M.gguf",
    sha256: "fd3e90605e8103307dca37cb5a8cdb036267e2fe3cb2d908d80a8ceb9ec0638c",
    size_bytes: 6_486_448_288,
    min_ram_gb: 8,
};

pub const CLEF_27B: ModelSpec = ModelSpec {
    id: "clef-27b",
    label: "Clef 27B (optional)",
    hf_repo: "ggml-org/Clef-GGUF",
    hf_file: "Clef-Q4_K_M.gguf",
    sha256: "bd9b8ae24c5752a2bb5e2733d5168340d5f04457e02b8bf0d1bc9285ec3d4f61",
    size_bytes: 19_232_219_200,
    min_ram_gb: 20,
};

pub const KEV_4B: ModelSpec = ModelSpec {
    id: "kev-4b",
    label: "Kev-4B (fallback)",
    hf_repo: "ggml-org/Kev-4B-GGUF",
    hf_file: "Kev-4B-Q4_K_M.gguf",
    sha256: "33ae6b18926502b2209a1bf7d3b61a350d65938441515c686bb87d226a19eff9",
    size_bytes: 3_033_489_824,
    min_ram_gb: 6,
};

pub static REGISTRY: &[ModelSpec] = &[CLEF_FLASH, CLEF_27B, KEV_4B];
