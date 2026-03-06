# PROVIDER CRATE

LLM provider abstraction layer. Two implementations (Anthropic, OpenAI) + OpenAI-compatible factory for 10+ providers. SSE streaming, model metadata database.

## FILES

| File | Purpose |
|------|---------|
| `src/provider.rs` | `LlmProvider` trait: `chat()` + `stream()`. Core types: `ChatRequest`, `ChatResponse`, `StreamEvent`, `ToolDefinition`, `ContentPart`, `ToolCall`, `Usage`, `FinishReason` |
| `src/providers/anthropic.rs` | Anthropic Messages API. SSE events: `message_start`, `content_block_start/delta/stop`, `message_delta/stop`. Supports system caching |
| `src/providers/openai.rs` | OpenAI Chat Completions API. `new_compatible(name, base_url, key)` for Groq/Together/Fireworks/DeepSeek/Mistral/xAI. Reasoning effort for o-series |
| `src/init.rs` | Factory: `parse_model_str("provider/model")` → provider instance. `find_api_key()` checks env. `OPENAI_COMPATIBLE` list maps providers to base URLs |
| `src/sse.rs` | SSE parser: `parse_sse()` → `Stream<SseEvent>`. Line-by-line with `[DONE]` sentinel. Used by both providers |
| `src/models_db.rs` | `ModelsDb` singleton: loads from cache (`~/.cache/opencoder/models.json`) or embedded snapshot. `refresh()` fetches models.dev API |
| `src/model.rs` | `Model` metadata: capabilities (temperature, reasoning, toolcall), cost (input/output/cache), limits (context/max_output), status |
| `src/error.rs` | `ParsedError`: `ContextOverflow` (triggers compaction), `ApiError` (with retryable flag) |

## SUPPORTED PROVIDERS

| Provider ID | Env Var | Implementation |
|-------------|---------|----------------|
| `anthropic` | `ANTHROPIC_API_KEY` | Native (anthropic.rs) |
| `openai` | `OPENAI_API_KEY` | Native (openai.rs) |
| `groq` | `GROQ_API_KEY` | OpenAI-compatible |
| `openrouter` | `OPENROUTER_API_KEY` | OpenAI-compatible |
| `together` | `TOGETHER_API_KEY` | OpenAI-compatible |
| `fireworks` | `FIREWORKS_API_KEY` | OpenAI-compatible |
| `deepseek` | `DEEPSEEK_API_KEY` | OpenAI-compatible |
| `mistral` | `MISTRAL_API_KEY` | OpenAI-compatible |
| `xai` | `XAI_API_KEY` | OpenAI-compatible |
| `azure` | `AZURE_OPENAI_API_KEY` | OpenAI-compatible |
| `copilot` | `COPILOT_API_KEY` | OpenAI-compatible |

## StreamEvent FLOW

```
Provider.stream() yields:
  TextDelta { content }          → Text part in session
  ReasoningDelta { content }     → Reasoning part
  ToolCallStart { id, name }     → Init pending tool
  ToolCallDelta { id, arguments} → Accumulate JSON args
  ToolCallEnd { id }             → Parse args, set Pending
  StepFinish { reason, usage }   → Step marker + token counts
  Error { message }              → Error propagation
```

## WHEN MODIFYING

- Adding OpenAI-compatible provider → add entry to `OPENAI_COMPATIBLE` in `init.rs` + env key to `PROVIDER_ENV_KEYS`
- Adding native provider → new file in `providers/`, implement `LlmProvider` trait, add match arm in `build_provider()`
- Updating model DB → run `scripts/download_models.sh` before release
- Model string inference → update `infer_provider()` match arms in `init.rs` (e.g. "claude" → anthropic)
