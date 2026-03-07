# Tweet Filtering Pipeline - Questionnaire (Claude Version)

## High-Level Goals

**1. What's the primary problem you're solving?**
- Are you drowning in too many tweets and want to see only the important ones?
- Do you want different "views" of your feed for different contexts (focus mode, casual browsing, research)?
- Something else?

**2. What does success look like?**
When you open your Twitter feed in the morning, what do you want to see? Walk me through an ideal scenario.

---

## Filtering Categories

**3. Let's define your categories more precisely:**

You mentioned:
- Engagement bait (e.g., "RT this if...", "Thread 🧵 1/47")
- Politics (all politics, or specific types?)
- Irrelevant content (how do you define relevance to you?)
- Disliked content (tweets you actively don't want, vs just low priority?)

For each category:
- Should it be **completely hidden**, **deprioritized**, or **tagged but visible**?
- Do you want to see what was filtered out, or trust the system to remove it?

**4. What categories am I missing?**
- Ads/promotional content?
- News vs commentary vs personal stories?
- Technical content vs general interest?
- Specific topics you always/never want to see?
- Quality signals (well-researched threads vs hot takes)?

---

## Topic Grouping

**5. When you say "group common tweets about the same topic":**
- Do you want to see ONE representative tweet from a trending topic instead of 50 similar takes?
- Or do you want all tweets about a topic grouped together so you can dive deep if interested?
- How granular? ("AI" as one topic, or "GPT-4 vs Claude" as separate from "AI regulation"?)

**6. What happens to grouped tweets?**
- Show the "best" one and hide the rest?
- Show a summary like "15 tweets about X"?
- Create a digest/summary of what people are saying?

---

## Personalization

**7. How does the system learn what's relevant to YOU?**
- Do you want to explicitly label tweets as "more like this" / "less like this"?
- Should it infer from your engagement (what you click, upvote in your UI)?
- Do you have specific topics/accounts that are always relevant?
- Should it learn from your past preferences over time?

**8. Do you want different filter profiles?**
- "Focus mode" - only high-quality, relevant content
- "Discovery mode" - include weird stuff, serendipity
- "Quick scan" - just headlines/summaries
- "Deep dive [topic]" - everything about AI safety, nothing else

---

## Technical Approach

**9. When does filtering happen?**
- Real-time as tweets come in?
- Batch process overnight and serve pre-filtered feed?
- On-demand when you load the UI?

**10. OpenRouter + free LLM:**
- Are you okay with the latency of LLM calls?
- How many tweets/day are we talking about? (Rate limits, costs)
- Should we batch tweets to reduce API calls?
- Do you want to cache classifications or re-evaluate each time?

**11. Pipeline architecture - which feels right?**

**Option A**: Tweet → LLM classifies (engagement bait? politics? relevant?) → Filter/tag → Store classifications

**Option B**: Tweet → Simple rules filter obvious stuff → LLM only for ambiguous cases → Store

**Option C**: Batch mode: Collect day's tweets → LLM groups by topic → LLM picks best from each group → Serve filtered feed

---

## Output & Consumption

**12. How do you actually want to consume the filtered feed?**
- Still see individual tweets (just fewer of them)?
- Topic-based digest: "Here are 3 main topics from your feed today..."
- Narrative summary: "Today your feed discussed X, Y, Z. Notable takes: ..."
- Something else?

**13. What about the filtered-out content?**
- Gone forever?
- Available in "Show filtered tweets" section?
- Weekly review of what you missed?
