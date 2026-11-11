# Running Margin on a local model, fast

Margin can use any model served on your own machine (Settings → Coach agent → *Local model*). Nothing leaves the Mac and nothing is metered. This page is about making that fast enough to live with. Everything here is your environment, not the app; Margin just connects to the URL you give it.

## 1. Know where the time goes

A coach turn is three phases:

1. **Loading the model** into memory, if it was unloaded. 10–40 s, and much longer if the Mac is swapping.
2. **Reading the prompt** (prompt processing). Margin sends the coach's instructions, your document and the conversation; a review is 4–9k tokens. On an M1 Max a 12B model reads about 300–500 tokens/s, a 26B about half that.
3. **Writing the answer**. A review is 300–900 tokens. A 12B writes about 25–35 tokens/s on an M1 Max, a 26B about 10–15.

Margin already keeps the model loaded for an hour after a turn, turns off the model's hidden "reasoning" phase (which otherwise burns a minute before the first word), and sends only what each action needs. The rest is below.

## 2. Memory is the whole game

A model must fit in RAM next to everything else you run, or every token comes from swap.

| Model | Needs about |
|---|---|
| gemma4:12b-it-qat | 8 GB |
| gemma4:26b | 17 GB |
| qwen3-coder:30b (q4) | 19 GB |
| gpt-oss:20b | 14 GB |

Check before a session: `ollama ps` shows what is loaded; Activity Monitor → Memory shows what is free. On a 32 GB Mac with a browser, Slack and an IDE open, a 26B model is already at the edge; close what you can, or use the 12B for the conversational turns and the 26B (or Claude) for the panel review.

## 3. Pick the model for the job

- **Conversation** (interview, thesis, fix English, lint): a 12B is plenty and answers in 5–30 s.
- **Judgement** (Review document, Read as my reader, Rehearse): a 20B+ model, or a hosted one. Smaller models follow the review format less reliably; Margin verifies every quote, so a weak model gives you fewer notes rather than wrong ones.
- Quantised variants (`-qat`, `q4_K_M`) are 30–40 % smaller and a little faster at a small cost in quality. Prefer them over a bigger model that does not fit.

Margin's *Quick coach* / *Deep coach* settings let you split exactly this way, including local for quick and Claude for deep.

## 4. Ollama settings worth changing

Ollama's defaults favour a laptop that does many things. For a coaching session:

```sh
# keep models loaded for an hour instead of five minutes (Margin asks for this per request; this makes it the default)
launchctl setenv OLLAMA_KEEP_ALIVE 1h

# let Ollama use more of the GPU memory (Apple Silicon shares RAM and GPU memory)
launchctl setenv OLLAMA_FLASH_ATTENTION 1
launchctl setenv OLLAMA_KV_CACHE_TYPE q8_0     # halves the memory of the prompt cache, no visible quality loss
```

Then quit and reopen Ollama.app. Flash attention and the q8 KV cache matter most for long prompts, which Margin's are.

Keep the context window at the default (32k for gemma4). Margin's prompts never exceed ~12k tokens; a bigger window only costs memory.

## 5. llama.cpp instead of Ollama

llama.cpp is what runs underneath Ollama. Driving it directly removes a layer and exposes more knobs. It is installed with `brew install llama.cpp`, and it can load the model files Ollama already downloaded (they are plain GGUF):

```sh
# find the blob for a model you pulled with Ollama
python3 - <<'EOF'
import json, glob
for m in glob.glob(f"{__import__('os').environ['HOME']}/.ollama/models/manifests/registry.ollama.ai/library/*/*"):
    for l in json.load(open(m))["layers"]:
        if l["mediaType"].endswith("model"):
            print(m.split("library/")[1], "→ ~/.ollama/models/blobs/" + l["digest"].replace(":", "-"))
EOF

# serve it on port 8080 with everything on the GPU, flash attention on, a 16k context
llama-server -m ~/.ollama/models/blobs/sha256-… --port 8080 -ngl 99 -fa on -c 16384 --alias gemma4-12b \
  --chat-template-kwargs '{"enable_thinking":false}'   # otherwise a thinking model reasons silently for a minute first
```

In Margin: Settings → Local model → Server URL `http://localhost:8080`, then pick the model. Measured on an M1 Max with gemma4 12B (same file, same review prompt): see the table at the end of this page.

### Speculative decoding

The biggest single speed-up for *writing* is speculative decoding: a tiny "draft" model guesses the next few tokens and the big one only verifies them. It needs a draft model of the same family and tokenizer:

```sh
llama-server -m <big model .gguf> -md <small draft .gguf> --port 8080 -ngl 99 -ngld 99 -fa on -c 16384 --draft-max 16 --draft-min 4
```

Gains of 1.3–2× on the writing phase are typical for prose; nothing on prompt reading. It only works with a matching draft (for example Qwen3 30B with Qwen3 0.6B as draft); gemma4 has no small sibling at the time of writing. If you want this, pick a family that has one.

## 6. Hardware

- **Apple Silicon unified memory** is the constraint, not the GPU. A 64 GB Mac runs a 26B model with room to spare and a 70B at q4; a 32 GB Mac is a 12B–26B machine.
- Generation speed scales with **memory bandwidth** (M1 Max 400 GB/s, M2/M3 Ultra 800 GB/s), prompt reading with GPU compute. A Max chip is roughly twice a Pro; an Ultra roughly twice a Max.
- A separate machine on the LAN (a Linux box with a 24 GB GPU, or a Mac Studio) works unchanged: run Ollama or llama.cpp there and give Margin its URL (`http://studio.local:11434`). Your document goes over your own network only.

## 7. Measure, don't guess

Ollama prints timings per request in `~/.ollama/logs/server.log` (`prompt_eval_duration`, `eval_duration`, `eval_count`). llama-server prints the same to its console. Tokens per second in the *eval* phase is the number that tells you whether a change helped. Margin's own thread shows the wall-clock time of every turn next to the reply.

## Measured on this author's M1 Max (32 GB)

Same document, same "Review this section" prompt, gemma4 12B (`it-qat`), model already loaded:

| Server | Time to first word | Total | Notes |
|---|---|---|---|
| Ollama 0.40, OpenAI endpoint, thinking on | ~60 s | 81 s | silent reasoning before the first word |
| Ollama 0.40, native API, thinking off | ~5 s | 31 s | what Margin uses since 0.6.3 |
| llama.cpp 0.8 (same GGUF), `-ngl 99 -c 16384`, thinking on | ~25 s | 119 s | prompt 181 tok/s, writing 21 tok/s; it wrote 2,096 tokens because the model's reasoning was left on |

Takeaways from the numbers: Ollama's native API with thinking off is the fastest path on this machine, and llama.cpp is not faster for the same file unless you also turn thinking off (`--chat-template-kwargs '{"enable_thinking":false}'` on `llama-server`) and keep the model warm; expect it to land within ±20 % of Ollama. The order of what matters: thinking off (3×), memory headroom (up to 3×), model size (2× between 12B and 26B), server choice (±20 %).
