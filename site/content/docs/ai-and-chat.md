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

When the active language is not Japanese (or an override already exists), a **Model override** field pins a model for that language; blank means the provider's default model. The rationale is stated right in the UI: a local model that handles Japanese fine may write terrible Koine — dead languages need stronger models, and a cloud model is recommended for Koine Greek.

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

Production is a chat with a persona defined by the active language — a native speaker for living languages; a dead language discloses a synthetic persona up front, since no native speakers exist to imitate. The design separates conversation from correction:

- **The partner converses, never corrects.** It replies only in the language you are practicing, reacts, asks follow-ups, and responds to what you meant — corrections never appear inside its replies.
- **Corrections come back as a paper-style write-up** of your own messages. Each model call returns both the reply and a set of annotations on your latest message, rendered as colored underlines:

| Underline | Meaning |
|---|---|
| Red | Grammatically wrong |
| Orange | Correct but unnatural or clunky |

Hover an underline to read the note (a short English explanation with the natural alternative). Clicking an underlined word opens the right panel with the dictionary entry and the write-up note stacked together.

Annotations are anchored by exact quotes from your message. A quoted span the model invents (one that does not appear verbatim in what you wrote) is dropped rather than guessed at — no underline is better than a wrong one. A message with nothing to flag gets no underlines.

For a dead language, *unnatural* means *unattested*: the Koine Greek pack tells the model to judge your writing against attested usage in the period's texts — the New Testament, the Septuagint, the Apostolic Fathers, documentary papyri — and to prefer "not attested in this period" over appeals to native intuition.

### Structured exercises

Two buttons above the input box start exercises through the ordinary chat pipeline, so corrections come back as the same paper-style write-up:

- **✍ Composition exercise** asks the partner for a short writing topic — stated in the practice language with a brief English hint — and your attempt gets the usual write-up.
- **⇄ Translation drill** takes a sentence from your own reading, has the partner translate it to natural English, asks you to translate that English back without looking, then compares your version with the original. The button is disabled until your library has sentences in the active language.

### Clickable words

Every message — yours and the partner's — runs through the same analysis pipeline as the reader: conjugated phrases group together, and clicking any word in the practice language opens the right panel with its dictionary entry — plus, for Japanese, the reading and conjugation summary. This works in pack languages too, through Tier-1 analysis — tokens resolve through the pack's full-form table when unambiguous. The input hint follows suit, reading "Write in *language*…" when the practice language is not Japanese. From there, **Learn** adds the word to your reviews, and **Known** / **Ignore** set its status, exactly as in the reader. See [Reviews & SRS](@/docs/reviews-and-srs.md) for what happens next.

### Level calibration

The partner's level is calibrated from three signals:

1. **Recorded vocabulary** — your known-word count in the active language seeds the prompt (for Japanese it maps to a rough JLPT band).
2. **Your own writing** — the model is told this estimate may lag reality and that your actual messages are the better signal, so a small recorded vocabulary never caps the conversation.
3. **The challenge dial** — a dropdown next to the input box: *Match my level*, *Push me a little* (the default), or *Full immersion* (natural, unrestricted text in the practice language — no simplification). Changing it saves immediately and applies from your next message.

### Conversations

The left sidebar lists past conversations; hovering shows the full title, message count, and start date. Conversations persist in the database — reopen one to continue it, or delete it with the trash button. **New conversation** starts a fresh thread.

In the input box, Enter sends and Shift+Enter inserts a newline.

## Privacy

Only the text needed for the feature leaves the app, and only to the provider you configured: the sentence being explained, or the conversation history plus the level hint described above. Nothing else — no library contents, no statistics, no review data — is ever sent, except that a translation drill, when you start one, includes the single sentence being drilled. With Ollama (or a local custom endpoint such as LM Studio), nothing leaves your machine at all.