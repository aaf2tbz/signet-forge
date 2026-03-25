# Auth and Model Discovery

Forge now treats auth and model availability as part of the Signet pipeline instead of as separate manual config.

## Sources of provider connectivity

Forge discovers providers from:

1. environment variables
2. Forge local credentials
3. Signet secrets
4. authenticated CLI tools
5. Ollama

For API providers, Signet secret names like `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` are imported into Forge automatically when Forge connects to the daemon.

## Signet secret sync

On startup, Forge:

1. connects to the Signet daemon
2. lists available Signet secrets
3. imports matching provider API keys into Forge local credentials if Forge does not already have a local key
4. requests a Signet model-registry refresh

This means a key stored once in Signet can make a provider available in Forge without a second manual setup step.

## CLI provider detection

Forge now only treats CLI providers as connected when they are actually authenticated:

- `claude-cli`: checked through `claude auth status --json`
- `codex-cli`: checked through `codex login status`
- `gemini-cli`: detected from saved/env auth

Installed-but-not-logged-in CLIs should no longer appear as fully connected providers.

## Model picker behavior

The model picker is filtered to connected providers.

- disconnected providers are hidden
- registry models from Signet are preferred when available
- fallback model lists are only used for connected providers that do not yet have full registry coverage

Provider-family mapping:

- `claude-code` registry entries are shown under `claude-cli`
- `codex` registry entries are shown under `codex-cli`

## Version auto-updates

Forge does not need every model version hardcoded locally anymore.

When Signet’s registry learns about newer model versions, Forge can surface them automatically for connected providers. This is already strongest for providers where Signet has live discovery support, and it improves further as the Signet registry gains broader provider coverage.
