//! jieger keyword auto-reply engine (comment -> knowledge -> danmaku).
//!
//! Pure scoring/answering lives here so it is unit-testable without a
//! browser, a comment listener, or a database. The two runtime seams that
//! belong to other tasks are traits only:
//!
//! - [`GoodsKnowledgeSource::list_goods_knowledge`] — implemented later by
//!   shop-product-script; the engine only reads the snapshot.
//! - [`DanmakuSender::send_danmaku`] — seam for `sub_account::send_danmaku`.
//!   This module never implements comment-listener or danmaku sending.
//! - [`AiReplyHook::request_ai_reply`] — MCP hook branch (`source=Ai`).
//!   `aiChat` stays in MCP (architecture constraint), so this crate only
//!   marks the branch and never performs a real AI call.
//!
//! Backpressure: the consumer subscribes to comment-listener's broadcast
//! channel. `tokio::broadcast` drops the oldest buffered events on overflow,
//! so a slow consumer observes `Lagged(n)` instead of unbounded growth. The
//! consumer logs the lag count and resyncs on the next event. Size the
//! channel with [`REPLY_CHANNEL_SIZE`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Recommended capacity for the comment-listener broadcast channel feeding
/// [`run_auto_reply_consumer`]. Overflow drops oldest (lag), never blocks
/// the producer.
pub const REPLY_CHANNEL_SIZE: usize = 64;

/// Per-token hit bonus in [`score_knowledge`].
pub const TOKEN_HIT_SCORE: i32 = 10;
/// Q&A hit bonus in [`score_knowledge`] (applied once per goods).
pub const QA_HIT_SCORE: i32 = 30;
/// Price-intent bonus in [`score_knowledge`].
pub const PRICE_INTENT_SCORE: i32 = 8;
/// Usage-intent bonus in [`score_knowledge`].
pub const USAGE_INTENT_SCORE: i32 = 5;

/// Intent names returned by [`detect_intent`].
pub const INTENT_PRICE: &str = "price";
pub const INTENT_PROMOTION: &str = "promotion";
pub const INTENT_USAGE: &str = "usage";
pub const INTENT_STOCK: &str = "stock";

/// Intent keyword constants, matched as substrings against normalized text.
/// First match wins in [`detect_intent`] (price > promotion > usage > stock).
const PRICE_PATTERNS: &[&str] = &["价格", "多少钱", "多少米", "报价", "售价", "贵"];
const PROMOTION_PATTERNS: &[&str] = &["优惠", "折扣", "满减", "券", "活动", "促销", "特价", "便宜"];
const USAGE_PATTERNS: &[&str] = &["怎么用", "用法", "使用", "教程", "功效", "怎么吃", "怎么喝", "方法"];
const STOCK_PATTERNS: &[&str] = &["库存", "有货", "没货", "缺货", "现货", "发货"];

/// Fallback suggestion text for the `Template` branch. It is returned to the
/// caller for UI display and is NEVER auto-sent as danmaku — the consumer
/// only sends on a knowledge hit.
pub const TEMPLATE_FALLBACK: &str = "收到，主播稍后为你解答";

// --- Knowledge model -------------------------------------------------------

/// One goods' reply knowledge. Produced later by shop-product-script via
/// [`GoodsKnowledgeSource`]; the engine treats it as an opaque snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodsKnowledge {
    pub goods_id: String,
    pub title: String,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub promotion: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub highlights: Vec<String>,
    /// Matchable tokens (goods name fragments, keywords). Each hit +10.
    #[serde(default)]
    pub tokens: Vec<String>,
    /// Exact Q&A pairs. A hit answers verbatim with priority (+30).
    #[serde(default)]
    pub qa: Vec<GoodsQa>,
}

/// One exact question/answer pair inside [`GoodsKnowledge`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodsQa {
    pub question: String,
    pub answer: String,
}

/// Knowledge source seam for shop-product-script. Sync snapshot read: the
/// consumer calls it per comment event, so implementations must be cheap
/// (cached) and never block on network.
pub trait GoodsKnowledgeSource: Send + Sync {
    fn list_goods_knowledge(&self) -> Vec<GoodsKnowledge>;
}

// --- Reply model -----------------------------------------------------------

/// Where a reply came from. `Ai` is a hook-only branch (no real AI call in
/// this crate); `Template` is a UI suggestion, never auto-sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplySource {
    Knowledge,
    Ai,
    Template,
}

/// Resolution outcome for one comment. `ok` means "a reply was produced and
/// (for knowledge hits) sent"; the consumer auto-sends only when
/// `knowledge_hit` is true.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyResult {
    pub ok: bool,
    pub reply: Option<String>,
    pub source: ReplySource,
    pub goods_id: Option<String>,
    pub intent: Option<String>,
    pub error: Option<String>,
    pub knowledge_hit: bool,
}

// --- Comment event + runtime seams ------------------------------------------

/// Minimal comment event carried by comment-listener's broadcast channel.
/// Defined here (not implemented here): comment-listener owns production.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentEvent {
    pub account_id: String,
    pub content: String,
}

/// Danmaku seam for `sub_account::send_danmaku`. Implemented elsewhere.
#[async_trait::async_trait]
pub trait DanmakuSender: Send + Sync {
    async fn send_danmaku(&self, account_id: &str, content: &str) -> Result<(), String>;
}

/// MCP hook branch for `source=Ai`. `aiChat` stays in MCP, so this crate
/// never performs a real AI call: return `Some(text)` to reply via AI,
/// `None` to decline (the consumer then records an `Ai` deferred miss).
pub trait AiReplyHook: Send + Sync {
    fn request_ai_reply(&self, question: &str) -> Option<String>;
}

/// Persistence seam for `auto_reply_records`落库. The profile-manager-backed
/// implementation is wired when comment-listener lands (PM lives on the
/// launcher thread); the consumer only depends on this trait.
pub trait ReplyRecorder: Send + Sync {
    fn record(&self, account_id: &str, content: &str, result: &ReplyResult);
}

// --- Engine -----------------------------------------------------------------

/// Normalize a comment/question: lowercase + strip all whitespace.
///
/// `to_lowercase` is a no-op for CJK (verified by test), so Chinese matching
/// is unaffected; full-width spaces (U+3000) count as whitespace and are
/// removed.
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Detect reply intent from (un-normalized) text. Returns one of the
/// `INTENT_*` names, or `None` when no intent keyword matches.
pub fn detect_intent(text: &str) -> Option<&'static str> {
    let q = normalize(text);
    if PRICE_PATTERNS.iter().any(|k| q.contains(k)) {
        return Some(INTENT_PRICE);
    }
    if PROMOTION_PATTERNS.iter().any(|k| q.contains(k)) {
        return Some(INTENT_PROMOTION);
    }
    if USAGE_PATTERNS.iter().any(|k| q.contains(k)) {
        return Some(INTENT_USAGE);
    }
    if STOCK_PATTERNS.iter().any(|k| q.contains(k)) {
        return Some(INTENT_STOCK);
    }
    None
}

/// Score one goods against a question: +10 per token hit, +30 once on any
/// Q&A hit, +8 on price intent, +5 on usage intent.
pub fn score_knowledge(question: &str, goods: &GoodsKnowledge) -> i32 {
    let q = normalize(question);
    if q.is_empty() {
        return 0;
    }
    let mut score = 0;
    for token in &goods.tokens {
        let t = normalize(token);
        if !t.is_empty() && q.contains(&t) {
            score += TOKEN_HIT_SCORE;
        }
    }
    if goods.qa.iter().any(|pair| {
        let nq = normalize(&pair.question);
        !nq.is_empty() && q.contains(&nq)
    }) {
        score += QA_HIT_SCORE;
    }
    match detect_intent(&q) {
        Some(INTENT_PRICE) => score += PRICE_INTENT_SCORE,
        Some(INTENT_USAGE) => score += USAGE_INTENT_SCORE,
        _ => {}
    }
    score
}

/// Build a reply from one goods: exact Q&A match wins verbatim, otherwise
/// assemble title/price/promotion/status/highlights. `None` when the goods
/// has nothing answerable (even a positive score yields no reply then).
pub fn answer_from_knowledge(question: &str, goods: &GoodsKnowledge) -> Option<String> {
    let q = normalize(question);
    // 1. Q&A exact priority: the question contains a known question.
    for pair in &goods.qa {
        let nq = normalize(&pair.question);
        if !nq.is_empty() && q.contains(&nq) {
            return Some(pair.answer.clone());
        }
    }
    // 2. Assemble available fields.
    let mut parts = Vec::new();
    if !goods.title.trim().is_empty() {
        parts.push(goods.title.trim().to_string());
    }
    if let Some(price) = goods.price.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(format!("价格：{price}"));
    }
    if let Some(promo) = goods
        .promotion
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push(promo.to_string());
    }
    if let Some(status) = goods
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        parts.push(status.to_string());
    }
    let highlights: Vec<&str> = goods
        .highlights
        .iter()
        .map(|h| h.trim())
        .filter(|h| !h.is_empty())
        .collect();
    if !highlights.is_empty() {
        parts.push(format!("亮点：{}", highlights.join("、")));
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("；"))
}

/// Try the whole knowledge list: score each goods, take the highest scorer.
/// Empty knowledge is skipped (no reply); a non-positive best score or an
/// unanswerable best goods also yields `None`.
pub fn try_knowledge_reply(question: &str, goods_list: &[GoodsKnowledge]) -> Option<ReplyResult> {
    if goods_list.is_empty() {
        return None;
    }
    let intent = detect_intent(question).map(str::to_string);
    let mut best: Option<(&GoodsKnowledge, i32)> = None;
    for goods in goods_list {
        let score = score_knowledge(question, goods);
        if score > best.map_or(0, |(_, s)| s) {
            best = Some((goods, score));
        }
    }
    let (goods, _) = best?;
    let reply = answer_from_knowledge(question, goods)?;
    Some(ReplyResult {
        ok: true,
        reply: Some(reply),
        source: ReplySource::Knowledge,
        goods_id: Some(goods.goods_id.clone()),
        intent,
        error: None,
        knowledge_hit: true,
    })
}

/// Full resolution: knowledge first, then the AI hook branch, then the
/// template suggestion. Pure (no IO) so commands and tests share it.
pub fn resolve_reply(
    question: &str,
    goods_list: &[GoodsKnowledge],
    ai_hook: Option<&dyn AiReplyHook>,
) -> ReplyResult {
    if let Some(hit) = try_knowledge_reply(question, goods_list) {
        return hit;
    }
    let intent = detect_intent(question).map(str::to_string);
    if let Some(hook) = ai_hook {
        match hook.request_ai_reply(question) {
            Some(text) => {
                return ReplyResult {
                    ok: true,
                    reply: Some(text),
                    source: ReplySource::Ai,
                    goods_id: None,
                    intent,
                    error: None,
                    knowledge_hit: false,
                };
            }
            None => {
                return ReplyResult {
                    ok: false,
                    reply: None,
                    source: ReplySource::Ai,
                    goods_id: None,
                    intent,
                    error: Some("ai-hook-deferred".into()),
                    knowledge_hit: false,
                };
            }
        }
    }
    ReplyResult {
        ok: false,
        reply: Some(TEMPLATE_FALLBACK.into()),
        source: ReplySource::Template,
        goods_id: None,
        intent,
        error: Some("knowledge-miss".into()),
        knowledge_hit: false,
    }
}

// --- Consumer ---------------------------------------------------------------

/// Subscribe to comment-listener's broadcast channel and handle every
/// `CommentEvent`: normalize -> knowledge reply -> on hit, persist via
/// `recorder` and send via [`DanmakuSender`]. Misses are recorded but never
/// auto-sent. Exits when the sender side closes; lagged (dropped-oldest)
/// bursts are logged and resynced.
pub async fn run_auto_reply_consumer(
    mut rx: broadcast::Receiver<CommentEvent>,
    knowledge: Arc<dyn GoodsKnowledgeSource>,
    sender: Arc<dyn DanmakuSender>,
    ai_hook: Option<Arc<dyn AiReplyHook>>,
    recorder: Arc<dyn ReplyRecorder>,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                if event.content.trim().is_empty() {
                    continue;
                }
                let goods = knowledge.list_goods_knowledge();
                let result =
                    resolve_reply(&event.content, &goods, ai_hook.as_deref());
                recorder.record(&event.account_id, &event.content, &result);
                if result.knowledge_hit {
                    if let Some(reply) = result.reply.as_deref() {
                        if let Err(e) = sender.send_danmaku(&event.account_id, reply).await {
                            tracing::warn!(account = %event.account_id, error = %e, "auto-reply danmaku send failed");
                        }
                    }
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!(skipped = n, "auto-reply consumer lagged; resyncing");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn sample_goods() -> GoodsKnowledge {
        GoodsKnowledge {
            goods_id: "g1".into(),
            title: "山茶油".into(),
            price: Some("99元".into()),
            promotion: Some("满100减10".into()),
            status: Some("现货".into()),
            highlights: vec!["冷榨".into(), "有机".into()],
            tokens: vec!["山茶油".into(), "茶油".into()],
            qa: vec![GoodsQa {
                question: "保质期多久".into(),
                answer: "保质期18个月".into(),
            }],
        }
    }

    #[test]
    fn normalize_lowercases_and_strips_all_whitespace() {
        assert_eq!(normalize(" 多少 钱？ "), "多少钱？");
        assert_eq!(normalize("ABC 多少钱"), "abc多少钱");
        assert_eq!(normalize("多少钱"), "多少钱");
        assert_eq!(normalize("　全角空格　"), "全角空格");
        assert_eq!(normalize(""), "");
        // to_lowercase is identity for CJK: matching is unaffected.
        assert_eq!(normalize("山茶油HAVE"), "山茶油have");
    }

    #[test]
    fn detect_intent_branches() {
        assert_eq!(detect_intent("这个多少钱"), Some(INTENT_PRICE));
        assert_eq!(detect_intent("有优惠吗"), Some(INTENT_PROMOTION));
        assert_eq!(detect_intent("这个怎么用"), Some(INTENT_USAGE));
        assert_eq!(detect_intent("还有库存吗"), Some(INTENT_STOCK));
        assert_eq!(detect_intent("主播好"), None);
    }

    #[test]
    fn score_token_hit() {
        let g = sample_goods();
        // "山茶油" contains tokens 山茶油 + 茶油 = 20, no intent bonus.
        assert_eq!(score_knowledge("山茶油好吗", &g), 20);
    }

    #[test]
    fn score_qa_hit_adds_30() {
        let g = sample_goods();
        // QA hit (+30) on top of token hits for 保质期多久 (no token overlap).
        let bare = GoodsKnowledge {
            tokens: vec![],
            ..sample_goods()
        };
        assert_eq!(score_knowledge("保质期多久", &bare), 30);
        let _ = g;
    }

    #[test]
    fn score_price_and_usage_intent_bonus() {
        let bare = GoodsKnowledge {
            tokens: vec![],
            qa: vec![],
            ..sample_goods()
        };
        assert_eq!(score_knowledge("多少钱", &bare), PRICE_INTENT_SCORE);
        assert_eq!(score_knowledge("怎么用", &bare), USAGE_INTENT_SCORE);
        assert_eq!(score_knowledge("主播好", &bare), 0);
        assert_eq!(score_knowledge("", &bare), 0);
    }

    #[test]
    fn answer_qa_exact_takes_priority_over_assembly() {
        let g = sample_goods();
        assert_eq!(
            answer_from_knowledge("请问保质期多久？", &g).as_deref(),
            Some("保质期18个月")
        );
    }

    #[test]
    fn answer_assembles_all_fields() {
        let g = sample_goods();
        let reply = answer_from_knowledge("山茶油多少钱", &g).unwrap();
        for expect in ["山茶油", "99元", "满100减10", "现货", "冷榨", "有机"] {
            assert!(reply.contains(expect), "missing {expect} in {reply}");
        }
    }

    #[test]
    fn answer_none_when_goods_has_nothing_answerable() {
        let empty = GoodsKnowledge {
            goods_id: "e".into(),
            title: String::new(),
            price: None,
            promotion: None,
            status: None,
            highlights: vec![],
            tokens: vec!["词".into()],
            qa: vec![],
        };
        assert_eq!(answer_from_knowledge("词", &empty), None);
    }

    #[test]
    fn try_reply_picks_highest_score_and_skips_empty() {
        let low = GoodsKnowledge {
            goods_id: "low".into(),
            title: "低".into(),
            price: None,
            promotion: None,
            status: None,
            highlights: vec![],
            tokens: vec!["油".into()],
            qa: vec![],
        };
        let high = sample_goods();
        let hit = try_knowledge_reply("山茶油多少钱", &[low, high]).unwrap();
        assert!(hit.ok && hit.knowledge_hit);
        assert_eq!(hit.goods_id.as_deref(), Some("g1"));
        assert_eq!(hit.source, ReplySource::Knowledge);

        assert_eq!(try_knowledge_reply("山茶油多少钱", &[]), None);
        assert_eq!(try_knowledge_reply("主播好", &[sample_goods()]), None);
    }

    #[test]
    fn try_reply_none_when_best_score_has_no_answer() {
        // Price intent gives +8 but the goods has nothing to assemble.
        let bare = GoodsKnowledge {
            goods_id: "b".into(),
            title: String::new(),
            price: None,
            promotion: None,
            status: None,
            highlights: vec![],
            tokens: vec![],
            qa: vec![],
        };
        assert_eq!(try_knowledge_reply("多少钱", &[bare]), None);
    }

    #[test]
    fn resolve_miss_falls_back_to_template_without_ai_hook() {
        let result = resolve_reply("主播好", &[sample_goods()], None);
        assert!(!result.ok && !result.knowledge_hit);
        assert_eq!(result.source, ReplySource::Template);
        assert_eq!(result.reply.as_deref(), Some(TEMPLATE_FALLBACK));
    }

    struct DecliningHook;
    impl AiReplyHook for DecliningHook {
        fn request_ai_reply(&self, _question: &str) -> Option<String> {
            None // no real AI call: aiChat stays in MCP
        }
    }

    #[test]
    fn resolve_miss_with_declining_hook_marks_ai_branch() {
        let result = resolve_reply("主播好", &[], Some(&DecliningHook));
        assert_eq!(result.source, ReplySource::Ai);
        assert!(!result.ok && !result.knowledge_hit);
        assert!(result.error.is_some());
    }

    // --- Mock channel consumption -------------------------------------------

    struct MockSource(Vec<GoodsKnowledge>);
    impl GoodsKnowledgeSource for MockSource {
        fn list_goods_knowledge(&self) -> Vec<GoodsKnowledge> {
            self.0.clone()
        }
    }

    struct MockSender {
        sent: Mutex<Vec<(String, String)>>,
        notify: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    }
    #[async_trait::async_trait]
    impl DanmakuSender for MockSender {
        async fn send_danmaku(&self, account_id: &str, content: &str) -> Result<(), String> {
            let item = (account_id.to_string(), content.to_string());
            self.sent.lock().unwrap().push(item.clone());
            let _ = self.notify.send(item);
            Ok(())
        }
    }

    struct MockRecorder {
        records: Mutex<Vec<(String, ReplyResult)>>,
    }
    impl ReplyRecorder for MockRecorder {
        fn record(&self, account_id: &str, _content: &str, result: &ReplyResult) {
            self.records
                .lock()
                .unwrap()
                .push((account_id.to_string(), result.clone()));
        }
    }

    #[tokio::test]
    async fn consumer_sends_hit_skips_miss_and_empty_content() {
        let (tx, rx) = broadcast::channel::<CommentEvent>(REPLY_CHANNEL_SIZE);
        let (notify_tx, mut notify_rx) =
            tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        let sender = Arc::new(MockSender {
            sent: Mutex::new(Vec::new()),
            notify: notify_tx,
        });
        let recorder = Arc::new(MockRecorder {
            records: Mutex::new(Vec::new()),
        });
        let knowledge: Arc<dyn GoodsKnowledgeSource> =
            Arc::new(MockSource(vec![sample_goods()]));
        let sender_dyn: Arc<dyn DanmakuSender> = sender.clone();
        let recorder_dyn: Arc<dyn ReplyRecorder> = recorder.clone();

        let handle = tokio::spawn(run_auto_reply_consumer(
            rx, knowledge, sender_dyn, None, recorder_dyn,
        ));

        tx.send(CommentEvent {
            account_id: "a1".into(),
            content: "山茶油多少钱".into(),
        })
        .unwrap();
        tx.send(CommentEvent {
            account_id: "a1".into(),
            content: "主播好".into(),
        })
        .unwrap();
        tx.send(CommentEvent {
            account_id: "a1".into(),
            content: "   ".into(),
        })
        .unwrap();

        // Exactly one danmaku: the knowledge hit. The miss is recorded but
        // never sent; blank content is skipped entirely.
        let (account, reply) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            notify_rx.recv(),
        )
        .await
        .expect("hit danmaku")
        .expect("channel open");
        assert_eq!(account, "a1");
        assert!(reply.contains("99元"), "unexpected reply: {reply}");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), notify_rx.recv())
                .await
                .is_err(),
            "miss must not send danmaku"
        );
        assert_eq!(sender.sent.lock().unwrap().len(), 1);
        assert_eq!(recorder.records.lock().unwrap().len(), 2);

        drop(tx);
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("consumer exits on close")
            .expect("no panic");
    }

    #[tokio::test]
    async fn consumer_skips_reply_when_knowledge_empty() {
        let (tx, rx) = broadcast::channel::<CommentEvent>(REPLY_CHANNEL_SIZE);
        let (notify_tx, mut notify_rx) =
            tokio::sync::mpsc::unbounded_channel::<(String, String)>();
        let sender = Arc::new(MockSender {
            sent: Mutex::new(Vec::new()),
            notify: notify_tx,
        });
        let recorder = Arc::new(MockRecorder {
            records: Mutex::new(Vec::new()),
        });
        let knowledge: Arc<dyn GoodsKnowledgeSource> = Arc::new(MockSource(vec![]));
        let sender_dyn: Arc<dyn DanmakuSender> = sender.clone();
        let recorder_dyn: Arc<dyn ReplyRecorder> = recorder.clone();

        let handle = tokio::spawn(run_auto_reply_consumer(
            rx, knowledge, sender_dyn, None, recorder_dyn,
        ));
        tx.send(CommentEvent {
            account_id: "a1".into(),
            content: "多少钱".into(),
        })
        .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), notify_rx.recv())
                .await
                .is_err(),
            "empty knowledge must not reply"
        );
        assert_eq!(recorder.records.lock().unwrap().len(), 1);
        drop(tx);
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("consumer exits on close")
            .expect("no panic");
    }
}
