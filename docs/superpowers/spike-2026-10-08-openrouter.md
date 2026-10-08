# Spike: OpenRouter System One API (2026-10-08)

1. `GET /api/v1/models?output_modalities=decisions` → 16 decision models (incl. `openai/gpt-6-luna-decisions`, `typesafe/jev-1.13`, `inception/mercury-decide:free`). Fields used: `id`, `name`, `context_length`, `pricing.prompt` (USD per token, string). **Returns 200 even with an invalid key** — not usable for key validation.
2. `POST /api/v1/systemone` accepts our existing wire format unchanged (string `state`, `null` choice criteria) plus `model`. Response = local format + `model`, `usage`, `id`, `provider`. Fixture: `src-tauri/tests/fixtures/openrouter_systemone_response.json`.
3. Luna (`openai/gpt-6-luna-decisions`) works on `/api/v1/systemone`: 200, same shape.

Errors: `{"error":{"message":..,"code":N}}`. Unknown model → 400 `"Model nope/nope does not exist"`. Bad key → 401 `"User not found."`.
Key validation: `GET /api/v1/key` → 200 valid / 401 invalid.
