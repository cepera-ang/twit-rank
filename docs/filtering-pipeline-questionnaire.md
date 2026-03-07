# Tweet Filtering + Topic Clustering Questionnaire

Use this to clarify what the filtering + clustering pipeline should do before implementation.

## 1) Goals + Non-goals
- What is the primary goal? (pick up to 2)
  - Reduce noise / spam
  - Reduce political content
  - Reduce engagement bait
  - Personalize to your tastes
  - De-duplicate and summarize “same topic”
  - Other:
- What is explicitly out of scope (for now)?
- What does “success” look like (qualitatively + 1–2 measurable signals)?

## 2) Filtering Semantics (the meaning of “filtered”)
- Confirmed: “Filtered” means marked and either collapsed in-place or shown in a separate feed.
- Default UX:
  - Collapse in main feed by default? If yes: collapsed per tweet or per topic cluster?
  - Separate “Filtered” feed always available? Should it be ordered differently?
- Actions available for a filtered item:
  - Show once / always show this topic / always show this author
  - Mark as irrelevant / dislike / mute
  - Undo / reset to defaults
- Should filtering ever be hard-hidden (not visible anywhere) or always recoverable?

## 3) Topic Clustering (broad topics across many authors)
- What is a “topic” in practice?
  - Narrow (single news event) vs broad (ongoing theme)
  - How tolerant should clustering be to different viewpoints/framings?
- Granularity:
  - Prefer fewer big clusters or many smaller clusters?
  - Target cluster size range (e.g., 2–10, 5–25, 20+ tweets)?
- Time behavior:
  - Do topics decay/expire? After how long without new tweets?
  - Should a topic span days/weeks, or reset daily?
- Mixed-topic tweets:
  - Allow multi-topic membership or force a single best topic?
- Topic naming:
  - LLM-generated short title? How “editorial” is allowed to be?
  - Should naming reflect neutral description vs your preferences?

## 4) What to Classify (tweet-level vs topic-level)
- Labels should apply to:
  - Individual tweets only
  - Topics only
  - Both (tweet label + topic label derived/aggregated)
- When a tweet is in a disliked topic:
  - Always collapse? Downrank? Put into filtered feed only?

## 5) Label Taxonomy (what categories exist)
For each label, decide: default action (collapse/separate/downrank/none), and whether it’s user-overridable.

Start set (edit freely):
- Engagement bait (rage-bait, “reply and I’ll…”, “hot take”, “ratio”, etc.)
- Politics (domestic, international, elections, policy, culture war)
- Outrage / doom / catastrophizing
- Low-signal repost / aggregator account
- Ads / sponsorship / giveaways
- Crypto / trading / gambling
- Celebrity / sports (if unwanted)
- “Irrelevant to me” (personal preference bucket)
- “Disliked” (derived from your feedback)

Open design choices:
- Should “politics” be one label or multiple (e.g., elections vs policy vs culture war)?
- Do you want positive labels too (e.g., “high-signal”, “must-read”)?
- Do you want an “uncertain” bucket that defaults to not filtering?

## 6) Definitions for Tricky Labels
- What exactly counts as “engagement bait” for you? List 5–10 examples (links or paraphrases).
- What counts as “politics” (and what doesn’t)? Examples:
  - Company regulation? labor organizing? climate? geopolitics? social issues?
- What does “irrelevant” mean: subject matter, tone, author, repeated exposure, or all of the above?

## 7) Preference Model (how it personalizes)
- Preference knobs you want:
  - Mute author
  - Mute topic
  - Mute label (e.g., politics)
  - Downrank author/topic/label by weight
  - Boost author/topic/label by weight
- Learning from feedback:
  - Use likes/dislikes to auto-adjust weights?
  - How aggressive should auto-learning be?
  - Should it require explicit “Apply learning” confirmation?
- Cold start:
  - Start conservative (mark only) vs proactive (collapse aggressively)?

## 8) Explainability (“why was this filtered / clustered?”)
- Should the UI show:
  - Detected labels + confidence
  - Short “reason” string (heuristic vs LLM)
  - Topic title + a 1-line summary
- How much explanation is “too much” (privacy, clutter)?

## 9) OpenRouter / LLM Usage
- Which OpenRouter models are acceptable (free-only)? List your preferred ones if you have them.
- Latency tolerance:
  - Must be instant (precomputed only) vs can classify asynchronously?
- Failure mode if OpenRouter is down / rate-limited:
  - Heuristics-only fallback
  - Queue for later and show “pending”
  - Disable filtering temporarily
- Prompting style:
  - Strict JSON schema output required?
  - Do you want the LLM to emit a “topic signature” for clustering (keywords/entities) or only labels?

## 10) Caching + Privacy
- Where should LLM outputs be stored?
  - Local SQLite only (default)
  - Never persist LLM outputs (recompute each time)
- What should be sent to the LLM?
  - Full tweet text
  - Text + author + engagement stats
  - Also include linked domain / quoted tweet text?
- Any red lines (e.g., never send DMs, never send certain accounts)?

## 11) UI / Interaction Design
- Main feed:
  - Show topics as grouped cards with expandable children?
  - Or show normal feed with a topic badge + “view cluster” drawer?
- Filtered feed:
  - Should it be grouped by label, by topic, or chronological?
- Controls:
  - One-click “Mute topic”, “Mute author”, “Not politics”, “Stop showing bait”

## 12) Ranking Integration
- When something is labeled:
  - Collapse only (no scoring impact)
  - Also downrank (how much?)
- Should topic diversity be enforced (avoid 20 tweets from same topic in a row)?
- How should retweets/quotes behave within clustering?

## 13) Evaluation + Iteration
- What is the first minimal MVP you’d accept?
  - Example: “topic clustering + politics/bait labels + collapse UI + manual mute”
- What sample period should we test on (last day/week/month)?
- What manual review workflow do you want to refine rules/prompts?

