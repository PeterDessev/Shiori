+++
title = "AI & Chat"
weight = 6
+++

Shiori can use a large language model for two things: sentence explanations in the reader and conversation practice in the Production view. The LLM is optional — every other feature works without one.

## Providers

Configure a backend under **Settings → AI**. Three providers are supported:

| Provider | What you need | Notes |
|---|---|---|
| Anthropic | API key + model name | Key is stored locally in `settings.json` and sent only to the Anthropic API. Leave the field empty to use the `ANTHROPIC_API_KEY` environment variable instead. |
| Ollama (local) | Ollama installed and running | Models run entirely on your machine; nothing leaves it. |
| Custom endpoint | Base URL, model, optional API key | Any server speaking the OpenAI chat-completions dialect: LM Studio, llama.cpp server, vLLM, or a cloud provider. |

The bottom of the page shows the currently active backend, or "none" if the configuration is incomplete.

### Ollama details

- **Detection** — the settings page probes the server (default `http://localhost:11434`) and shows its status: running with version and installed-model count, or "not reachable" with a pointer to install Ollama from ollama.com. A refresh button re-probes.
- **Model picker** — when the server is reachable, a dropdown lists every installed model with its size on disk and parameter count.
- **In-app pulls** — type a model name (e.g. `qwen3:8b`) and press Pull; download progress streams into a progress bar in the settings page. Japanese-capable suggestions: qwen3, gemma3, llama3.1-swallow.
- **Fully offline** — once a model is pulled, explanations and chat work with no network at all.
- The first request after a cold start loads the model into memory and can take tens of seconds; this is normal.

The server URL field accepts a remote address too (e.g. an Ollama instance on another machine on your network).

## What the LLM powers

- **Sentence explanations** in the [Reading](@/docs/reading.md) view: an explanation of the current sentence's grammar and structure, on demand.
- **Conversation practice** in the Production view, described below.

If no backend is configured, the Production input area is replaced by a note pointing to Settings → AI.

## Production chat

Production is a chat with a native-speaker persona. The design separates conversation from correction:

- **The partner converses, never corrects.** It replies only in Japanese, reacts, asks follow-ups, and responds to what you meant — corrections never appear inside its replies.
- **Corrections come back as a paper-style write-up** of your own messages. Each model call returns both the reply and a set of annotations on your latest message, rendered as colored underlines:

| Underline | Meaning |
|---|---|
| Red | Grammatically wrong |
| Orange | Correct but unnatural or clunky |

Hover an underline to read the note (a short English explanation with the natural alternative). Clicking an underlined word opens the right panel with the dictionary entry and the write-up note stacked together.

Annotations are anchored by exact quotes from your message. A quoted span the model invents (one that does not appear verbatim in what you wrote) is dropped rather than guessed at — no underline is better than a wrong one. A message with nothing to flag gets no underlines.

### Clickable words

Every message — yours and the partner's — runs through the same morphological pipeline as the reader: conjugated phrases group together, and clicking any Japanese word opens the right panel with its reading, dictionary entry, and conjugation summary. From there, **Learn** adds the word to your reviews, and **Known** / **Ignore** set its status, exactly as in the reader. See [Reviews & SRS](@/docs/reviews-and-srs.md) for what happens next.

### Level calibration

The partner's Japanese is calibrated from three signals:

1. **Recorded vocabulary** — your known-word count maps to a rough JLPT band that seeds the prompt.
2. **Your own writing** — the model is told this estimate may lag reality and that your actual messages are the better signal, so a small recorded vocabulary never caps the conversation.
3. **The challenge dial** — a dropdown next to the input box: *Match my level*, *Push me a little* (the default), or *Full immersion* (natural native Japanese, no simplification). Changing it saves immediately and applies from your next message.

### Conversations

The left sidebar lists past conversations; hovering shows the full title, message count, and start date. Conversations persist in the database — reopen one to continue it, or delete it with the trash button. **New conversation** starts a fresh thread.

In the input box, Enter sends and Shift+Enter inserts a newline.

## Privacy

Only the text needed for the feature leaves the app, and only to the provider you configured: the sentence being explained, or the conversation history plus the level hint described above. Nothing else — no library contents, no statistics, no review data — is ever sent. With Ollama (or a local custom endpoint such as LM Studio), nothing leaves your machine at all.