# How to write a technical document people actually read

This is the standard the coach reviews against. It is short on purpose. Every rule here comes from the same question: *what does the reader need, in the order they need it?*

## 1. Decide who it is for, then write for them

Before you type a heading, answer three things in one line at the top of your notes:

- **Who reads this?** A staff engineer who will approve it. A new teammate who will maintain it. A manager who needs to decide. Pick the main one.
- **What do they already know?** Do not explain what your team already knows. Do explain what only you know.
- **What should they do after reading?** Approve, decide, build, debug, learn. Everything in the document serves that action.

Write for a smart person in a hurry who was not in the room. If they can skim the headings and the first line of each section and still get the point, you have written it well.

## 2. Lead with the point

- First paragraph: the problem, what you propose, and what you want from the reader. Three sentences is enough.
- Every section starts with its conclusion, then the reasons. Reviewers read top-down and stop when they have what they need.
- Put the detail in the body, the evidence in appendices, and the numbers wherever the claim is.

Bad: "In this document we explore several approaches to rate limiting." Good: "We propose a per-client token bucket in Redis. It stops a few clients from slowing everyone at month end. We need approval for the Redis cluster by Friday."

## 3. One idea per sentence, one topic per paragraph

- Short sentences. If a sentence has two "and"s or a semicolon, it is probably two sentences.
- Plain words. *Use*, not *utilise*. *Start*, not *initiate*. *Because*, not *due to the fact that*.
- Active voice with a real subject. "The job retries three times" beats "Retries are performed three times by the job". The reader needs to know who does what.
- Say the thing. "This may potentially cause some latency issues" means "This adds about 40 ms to every request".

## 4. Numbers beat adjectives

Reviewers do not trust "fast", "many", "slow", "often", "significant". They trust numbers with units and a source.

- "Latency is bad at month end" → "p99 latency rises from 200 ms to 2.1 s between the 28th and the 1st (dashboard link)."
- "Many clients" → "3 of 240 clients send 70% of requests."
- If you do not have the number yet, write `[TODO: p99 during the last spike]` rather than guessing. A visible gap is honest; a vague adjective hides the gap.

## 5. Show the thinking, not just the answer

A design is a decision. The reader wants to check the decision, so show what you compared:

- At least two real alternatives, including "do nothing". Say why each lost, in one or two lines. A list of strawmen convinces nobody.
- Name the trade-off you accepted. "We chose simplicity over exact fairness: a client can briefly exceed its limit by one bucket."
- Say what would change your mind. "If the Redis p99 is above 5 ms under load, we fall back to in-process limits."

## 6. Be honest about risk and uncertainty

- A risks section with "none" is a red flag. Every change has failure modes: wrong config, partial rollout, a dependency down, the data being different from what you assumed.
- Separate *known*, *assumed*, and *unknown*. "We assume clients retry with backoff (unverified)."
- Open questions with an owner and a date beat silent gaps.

## 7. Structure for skimming

- Headings are sentences a reader can act on, not labels. "Rollout: 1% of traffic for a week, then 10%" beats "Rollout".
- Use lists for parallel items, tables for comparisons, code for exact commands and payloads, diagrams for flows with more than three boxes.
- Define a term the first time you use it, or link to where it is defined. Expand acronyms once.
- Keep one tense and one name per thing. If it is "the limiter" in section 2, it is not "the throttling service" in section 5.

## 8. Match the document type

**Design doc** – problem with numbers → goals and non-goals → proposed design end to end → alternatives → risks → rollout and testing. The reader is approving a direction.

**RFC** – one-paragraph summary, the motivation, the precise proposal with examples, drawbacks, alternatives, unresolved questions. The reader is agreeing to a change that affects them.

**ADR** – context, options with pros and cons, the decision in one sentence, consequences. Short. Dated. The reader is a future teammate asking "why did we do this?"

**Incident postmortem** – summary, impact in numbers, timeline with time zone, root cause (keep asking why), what went well and badly, action items with owners and dates. Blameless: name systems and decisions, not people.

**Runbook** – the symptom, how to confirm it, the exact steps with commands, how to verify it worked, who to call. The reader is stressed and on call; no prose.

**Blog post or talk** – a hook that states the lesson, the concrete problem, what you did with real detail, what you measured, what you would tell your past self. The reader can leave at any time; earn each paragraph.

## 9. Before you send it

Read it once as the reader, not the writer:

1. Can I say the point of the document in one sentence? Is that sentence in the first paragraph?
2. Is every claim either a number with a source, a quoted requirement, or marked as an assumption?
3. Did I answer "why not the obvious alternative?"
4. What would the toughest reviewer ask first? Is the answer already here?
5. Is anything there only because it was hard to do, not because the reader needs it? Cut it.
6. Are the sentences short and the verbs active? Read one paragraph aloud.

## 10. On writing in a second language

Clarity is not about vocabulary. Short sentences in simple English are what good engineers write in every language. The coach fixes the grammar; your job is the thinking: the numbers, the alternatives, the risks, and the reader's next step. A plain, correct, specific document beats an elegant vague one every time.
