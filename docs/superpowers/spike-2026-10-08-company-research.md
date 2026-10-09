# Spike 2026-10-08: company research (agentix, LFM2.5, Exa, page text)

## agentix 0.31.0 — pin `agentix = "=0.31.0"`

- `agentix::agent(tools, http, request, history, history_budget)` runs the loop, but:
  - no round cap (only a token-budget truncation),
  - no hook to rewrite history between rounds,
  - malformed tool-call JSON is silently replaced with `{}` (`unwrap_or(json!({}))`), so the model never sees the error.
- **Decision:** drive rounds ourselves with `Request::complete(&http)`; history is a `Vec<agentix::Message>` we own.
- Building a round:
  `Request::openrouter(key).base_url(base).model(model).system_prompt(p).messages(history).tools(defs).complete(&http)`
  → `CompleteResponse { content, reasoning, tool_calls: Vec<ToolCall{id,name,arguments}>, provider_data }`.
- Push `Message::Assistant { content, reasoning, tool_calls, provider_data }`, then one `Message::ToolResult { call_id, content: vec![Content::Text{text}] }` per call.
- Tools: `ToolDefinition::function(agentix::raw::shared::FunctionDefinition { name, description, parameters, strict: None })`.
- **Use `Request::openrouter` for every endpoint.** `Provider::OpenAI` posts to `{base}/responses`, which llama-server does not serve; the OpenRouter provider posts to `{base}/chat/completions`.
- Cancellation: dropping the `complete` future aborts the HTTP request; wrap each round in `tokio::select!` with the `CancellationToken`.

## Endpoints verified

| Endpoint | Base URL | Result |
|---|---|---|
| llama-server (bundled binary) + LFM2.5, `--jinja -c 32768` | `http://127.0.0.1:{port}/v1`, empty key | search → `submit_profile` in 2 rounds, valid JSON args |
| OpenRouter, `deepseek/deepseek-v4-flash` | `https://openrouter.ai/api/v1` | tool calls and tool results round-trip |

- llama-server `GET /props` → `default_generation_settings.n_ctx` gives the context size.
- Chat sidecar args: decision args plus `--jinja -c 32768`.

## LFM2.5-2.6B GGUF

- `hf_repo: "LiquidAI/LFM2.5-2.6B-GGUF"`, `hf_file: "LFM2.5-2.6B-Q4_K_M.gguf"`
- `sha256: "02a8b7e17487d326e46d68ce0ba24211e1b80a14c4cd0597fa73c1cd697f52ed"`
- `size_bytes: 1_674_455_040`, `min_ram_gb: 4`

## Page text

- `html2text` 0.17 keeps nav menus and link references: noisy.
- **`dom_smoothie` 0.18** (Readability port): clean article text on stripe.com/about, nubank about-us, anthropic.com/company.
- Decision: `dom_smoothie` first, `html2text` fallback when Readability fails; collapse whitespace.

## Exa

- `POST https://api.exa.ai/search`, header `x-api-key`, body `{"query", "type":"auto", "numResults":5, "contents":{"text":{"maxCharacters":500}}}`.
- 200 → `results[]` with `title`, `url`, `text` (page text, capped by `maxCharacters`); also `id`, `image`, `entities` (unused).
- Invalid key → HTTP 401. Fixture: `src-tauri/tests/fixtures/exa_search_response.json`.
