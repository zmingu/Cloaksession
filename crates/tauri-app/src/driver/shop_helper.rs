//! 跟播助手上车 — `zs.kwaixiaodian.com/page/helper` 小黄车商品上车/下车。
//!
//! # 关键边界（架构硬约束）
//!
//! **与 CPS 加货架（`cps.kwaixiaodian.com`，开播前准备，手动粘贴文本解析）严格隔离。**
//! 本模块只认 `https://zs.kwaixiaodian.com/page/helper` 精确源（[`helper_page_url`]），
//! DOM helpers 全部以 [`HELPER_GOODS_DOM_HELPERS`] 内嵌进 `evaluate`（`HELPER_` 前缀，
//! 不复用、不引用任何 CPS 代码或选择器）。
//!
//! # DOM helpers 内嵌策略
//!
//! 快手 DOM 多变，`page.evaluate` 在浏览器内完成读取/点击比 locator 更鲁棒。
//! 每次调用由 Rust 包一层 IIFE 执行 helpers + 动作——CDP `evaluate`
//! 的顶层词法声明跨调用共享（重复 `const` 会报已声明），IIFE 内执行无此问题。
//!
//! # 写动作
//!
//! 上车/下车是真实商品上架写动作：点击前、点击后各校验一次页面仍是跟播助手页，
//! 且点击后重读验证状态翻转。真号验收单独确认。

use super::TauriBrowserDriver;
use cdp_driver::{TaskCancel, TaskError, TaskPage};
use multizen_core::{MultizenError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 跟播助手页地址。脚本化上车不负责导航：调用方（`ensure_auth` 流程）须先进入此页。
pub const HELPER_PAGE_URL: &str = "https://zs.kwaixiaodian.com/page/helper";

const LOCK_WAIT: Duration = Duration::from_millis(500);
const READ_WAIT: Duration = Duration::from_secs(8);
const WRITE_WAIT: Duration = Duration::from_secs(15);
const POST_CLICK_SETTLE: Duration = Duration::from_millis(800);
const TAB_SYNC_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PAGES_SCANNED: usize = 16;

/// 与 JS 侧 `HELPER_NOISE_TEXTS` 同源的噪声文本表（Rust 侧解码时二次过滤）。
/// [`tests`] 会断言本表每一项都出现在 [`HELPER_GOODS_DOM_HELPERS`] 中。
pub const HELPER_NOISE_TEXTS: &[&str] = &[
    "没有更多商品啦",
    "暂无数据",
    "暂无商品",
    "加载中",
    "上拉加载更多",
    "下拉刷新",
    "空空如也",
    "网络不太给力",
    "点击重试",
    "商品加载失败",
    "搜索商品",
    "请输入",
];

/// 浏览器内 DOM helpers：normalize / isVisible / readId 多策略 / NOISE_TEXTS /
/// isNoiseText + 商品读取、Tab 切换、按 ID 点击。每次调用由 Rust 包一层 IIFE。
pub const HELPER_GOODS_DOM_HELPERS: &str = r#"const HELPER_NOISE_TEXTS = ['没有更多商品啦','暂无数据','暂无商品','加载中','上拉加载更多','下拉刷新','空空如也','网络不太给力','点击重试','商品加载失败','搜索商品','请输入'];
const HELPER_IN_CART_TABS = ['小黄车','购物车','已上车','车内商品'];
const HELPER_TO_ADD_TABS = ['待上车','可选商品','全部商品','待添加'];
const HELPER_ON_LABELS = ['上车','加车','加入'];
const HELPER_OFF_LABELS = ['下车','移除','下架'];
const HELPER_SKIP_LINES = ['上车','下车','加车','移除','讲解','详情','预览','上架','下架'];
function helperNormalizeText(value) {
  return String(value == null ? '' : value).replace(/[\s\u00a0\u200b-\u200d\ufeff]+/g, ' ').trim();
}
function helperIsNoiseText(value) {
  const text = helperNormalizeText(value);
  if (!text) return true;
  return HELPER_NOISE_TEXTS.some(function (noise) { return text.indexOf(noise) !== -1; });
}
function helperIsVisible(el) {
  if (!el || el.nodeType !== 1) return false;
  const style = getComputedStyle(el);
  if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse') return false;
  const rect = el.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}
function helperActionOf(label) {
  const text = helperNormalizeText(label);
  if (!text) return null;
  if (HELPER_OFF_LABELS.some(function (k) { return text.indexOf(k) !== -1; })) return 'off';
  if (HELPER_ON_LABELS.some(function (k) { return text.indexOf(k) !== -1; })) return 'on';
  return null;
}
function helperReadId(root) {
  if (!root || root.nodeType !== 1) return null;
  const text = root.innerText || root.textContent || '';
  let m = /(?:^|[^A-Za-z0-9_])ID\s*[：:]?\s*(\d{1,32})(?!\d)/i.exec(text);
  if (m) return { id: m[1], via: 'text' };
  m = /goods[-_]?(\d{1,32})/i.exec(root.getAttribute('id') || '');
  if (m) return { id: m[1], via: 'id-attr' };
  const input = root.querySelector('input');
  const inputValue = helperNormalizeText(input && input.value);
  if (/^\d{1,32}$/.test(inputValue)) return { id: inputValue, via: 'input-value' };
  const dataset = root.dataset || {};
  const dataAttrs = [dataset.goodsId, dataset.goodsid, dataset.id, dataset.itemId, dataset.productId,
    root.getAttribute('data-goods-id'), root.getAttribute('data-id')];
  for (const candidate of dataAttrs) {
    const value = helperNormalizeText(candidate);
    if (/^\d{1,32}$/.test(value)) return { id: value, via: 'data-attr' };
  }
  const nested = root.querySelector('[data-goods-id],[data-goodsid],[data-id],[data-item-id]');
  if (nested && nested !== root) {
    const nestedDataset = nested.dataset || {};
    const nestedAttrs = [nestedDataset.goodsId, nestedDataset.goodsid, nestedDataset.id, nestedDataset.itemId,
      nested.getAttribute('data-goods-id'), nested.getAttribute('data-id')];
    for (const candidate of nestedAttrs) {
      const value = helperNormalizeText(candidate);
      if (/^\d{1,32}$/.test(value)) return { id: value, via: 'nested-data' };
    }
  }
  const link = root.querySelector('a[href]');
  m = /[?&#](?:goods_id|goodsId|itemId|productId|\bid\b)=(\d{1,32})/i.exec((link && link.getAttribute('href')) || '');
  if (m) return { id: m[1], via: 'nested-href' };
  return null;
}
function helperCardRoot(button) {
  let node = button;
  for (let depth = 0; depth < 6 && node && node !== document.body; depth += 1) {
    node = node.parentElement;
    if (!node) return null;
    if (helperReadId(node)) return node;
  }
  return null;
}
function helperCardName(root, buttons) {
  const skip = new Set(HELPER_SKIP_LINES);
  const buttonLabels = new Set(buttons.map(function (b) { return helperNormalizeText(b.textContent); }));
  const text = root.innerText || root.textContent || '';
  const lines = text.split('\n');
  for (const raw of lines) {
    const line = helperNormalizeText(raw);
    if (!line || helperIsNoiseText(line) || skip.has(line) || buttonLabels.has(line)) continue;
    if (line.length <= 60) return line;
  }
  return '';
}
const HelperGoods = {
  read: function (tab) {
    const url = location.href;
    const seen = new Map();
    const buttons = Array.prototype.slice.call(document.querySelectorAll('button,[role="button"],a,input[type="button"]'));
    for (const button of buttons) {
      if (!helperIsVisible(button)) continue;
      const action = helperActionOf(button.textContent);
      if (!action) continue;
      const root = helperCardRoot(button);
      if (!root) continue;
      const found = helperReadId(root);
      if (!found) continue;
      let entry = seen.get(found.id);
      if (!entry) {
        entry = { id: found.id, via: found.via, actions: [], buttons: [], root: root };
        seen.set(found.id, entry);
      }
      if (entry.actions.indexOf(action) === -1) entry.actions.push(action);
      entry.buttons.push(button);
    }
    const goods = [];
    for (const entry of seen.values()) {
      const rawText = helperNormalizeText(entry.root.innerText || entry.root.textContent || '').slice(0, 500);
      goods.push({ id: entry.id, via: entry.via, name: helperCardName(entry.root, entry.buttons), rawText: rawText, actions: entry.actions });
    }
    return { url: url, tab: tab, goods: goods, buttonCount: buttons.length };
  },
  switchTab: function (tab) {
    const url = location.href;
    const candidates = tab === 'inCart' ? HELPER_IN_CART_TABS : HELPER_TO_ADD_TABS;
    const els = document.querySelectorAll('button,a,[role="tab"],li,span,div');
    for (const el of els) {
      if (!helperIsVisible(el)) continue;
      const text = helperNormalizeText(el.textContent);
      if (!text || text.length > 12) continue;
      if (!candidates.some(function (k) { return text.indexOf(k) !== -1; })) continue;
      el.click();
      return { url: url, tab: tab, clicked: true, label: text };
    }
    return { url: url, tab: tab, clicked: false, label: '' };
  },
  act: function (goodsId, action) {
    const url = location.href;
    const want = action === 'on' ? HELPER_ON_LABELS : HELPER_OFF_LABELS;
    const buttons = document.querySelectorAll('button,[role="button"],a');
    for (const button of buttons) {
      if (!helperIsVisible(button)) continue;
      if (!want.some(function (k) { return helperNormalizeText(button.textContent).indexOf(k) !== -1; })) continue;
      const root = helperCardRoot(button);
      if (!root) continue;
      const found = helperReadId(root);
      if (!found || found.id !== goodsId) continue;
      button.click();
      return { url: url, goodsId: goodsId, action: action, clicked: true, label: helperNormalizeText(button.textContent).slice(0, 20) };
    }
    return { url: url, goodsId: goodsId, action: action, clicked: false, label: '' };
  }
};"#;

/// 跟播助手页 URL 守卫：精确 `https://zs.kwaixiaodian.com(:443)` 源、无 userinfo、
/// 路径为 `/page/helper`（允许其子路径与 query）。与 CPS（`cps.kwaixiaodian.com`）
/// 是不同源，天然隔离。
pub fn helper_page_url(raw: &str) -> bool {
    if raw.len() > 8192
        || raw.split_once("://").is_some_and(|(_, v)| {
            v.split(['/', '?', '#'])
                .next()
                .is_some_and(|v| v.contains('@'))
        })
    {
        return false;
    }
    reqwest::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("zs.kwaixiaodian.com")
            && u.port_or_known_default() == Some(443)
            && u.username().is_empty()
            && u.password().is_none()
            && (u.path() == "/page/helper" || u.path().starts_with("/page/helper/"))
    })
}

/// 商品状态。由 Rust 侧 [`infer_status`] 从动作按钮 + Tab + 文本推断（JS 只给原始动作）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HelperGoodStatus {
    Available,
    OnShelf,
    OffShelf,
    Unknown,
}

/// 商品 Tab：小黄车内商品 / 待上车商品。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HelperGoodTab {
    InCart,
    ToAdd,
}

impl HelperGoodTab {
    /// 解析前端/脚本传入的 Tab。接受 camelCase 与 snake_case。
    pub fn parse(raw: &str) -> std::result::Result<Self, String> {
        match raw.trim().to_ascii_lowercase().replace('_', "").as_str() {
            "incart" | "cart" => Ok(Self::InCart),
            "toadd" => Ok(Self::ToAdd),
            _ => Err("未知商品 Tab（inCart/toAdd）".into()),
        }
    }

    /// 传给 JS `HelperGoods` 的 Tab 键。
    pub fn key(self) -> &'static str {
        match self {
            Self::InCart => "inCart",
            Self::ToAdd => "toAdd",
        }
    }
}

/// 跟播助手商品信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperGoodInfo {
    pub goods_id: String,
    pub goods_name: String,
    pub raw_text: String,
    /// 可用动作（`上车`/`下车`子集），由 JS 原始动作映射。
    pub available_actions: Vec<String>,
    pub status: HelperGoodStatus,
    /// 读取来源 Tab（`inCart`/`toAdd`）。
    pub source_tab: String,
}

/// 上车/下车写动作结果（含操作后重读的商品列表）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperGoodActionResult {
    pub ok: bool,
    pub goods_id: String,
    /// `on`（上车）/`off`（下车）。
    pub action: String,
    pub detail: String,
    pub goods: Vec<HelperGoodInfo>,
}

fn shop_err(message: impl Into<String>) -> MultizenError {
    MultizenError::Mcp(message.into())
}

fn task_err(error: TaskError) -> MultizenError {
    match error {
        TaskError::Cancelled => shop_err("跟播助手操作已取消"),
        TaskError::TimedOut => shop_err("跟播助手页操作超时"),
        TaskError::Interrupted => shop_err("跟播助手页任务中断，请重新执行"),
        TaskError::InvalidOptions(reason) => shop_err(format!("跟播助手任务参数无效：{reason}")),
        TaskError::Driver(inner) => inner,
    }
}

/// 商品 ID 校验：纯数字、1–32 位。JS 各 readId 策略的输出统一收敛到此规则。
pub fn valid_goods_id(raw: &str) -> bool {
    !raw.is_empty() && raw.len() <= 32 && raw.bytes().all(|b| b.is_ascii_digit())
}

fn sanitize_name(raw: &str, goods_id: &str) -> String {
    let name: String = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect();
    if name.is_empty()
        || name.chars().any(char::is_control)
        || HELPER_NOISE_TEXTS.iter().any(|n| name.contains(n))
    {
        return goods_id.to_string();
    }
    name
}

/// 状态推断：显式文本 > 动作按钮 > Tab 默认值，无法判断时 `Unknown`（不编造）。
pub fn infer_status(actions: &[String], tab: HelperGoodTab, raw_text: &str) -> HelperGoodStatus {
    if raw_text.contains("已上车") || raw_text.contains("已在车") {
        return HelperGoodStatus::OnShelf;
    }
    if raw_text.contains("已下架") || raw_text.contains("已下车") || raw_text.contains("已移除") {
        return HelperGoodStatus::OffShelf;
    }
    if actions.iter().any(|a| a == "下车") {
        return HelperGoodStatus::OnShelf;
    }
    if actions.iter().any(|a| a == "上车") {
        return match tab {
            HelperGoodTab::InCart => HelperGoodStatus::OnShelf,
            HelperGoodTab::ToAdd => HelperGoodStatus::Available,
        };
    }
    HelperGoodStatus::Unknown
}

#[derive(Debug, Deserialize)]
struct RawHelperGood {
    id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(rename = "rawText", default)]
    raw_text: String,
    #[serde(default)]
    actions: Vec<String>,
}

fn map_action(raw: &str) -> Option<&'static str> {
    match raw {
        "on" => Some("上车"),
        "off" => Some("下车"),
        _ => None,
    }
}

/// 解码 JS `HelperGoods.read` 返回：校验 URL 仍在跟播助手页，逐商品校验 ID、
/// 清洗名称、映射动作、推断状态。任一商品 ID 非法即整批拒绝（不静默丢弃）。
pub fn decode_goods_list(
    value: serde_json::Value,
    tab: HelperGoodTab,
) -> std::result::Result<Vec<HelperGoodInfo>, String> {
    let url = value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if !helper_page_url(url) {
        return Err("读取期间页面已离开跟播助手页".into());
    }
    let goods = value
        .get("goods")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "跟播助手页返回结构无效".to_string())?;
    let mut out = Vec::with_capacity(goods.len());
    for item in goods {
        let raw: RawHelperGood = serde_json::from_value(item.clone())
            .map_err(|_| "跟播助手页返回商品结构无效".to_string())?;
        let goods_id = raw.id.unwrap_or_default();
        if !valid_goods_id(&goods_id) {
            return Err(format!("跟播助手页返回商品 ID 无效：{goods_id}"));
        }
        let raw_text: String = raw.raw_text.chars().take(2000).collect();
        let actions: Vec<String> = raw
            .actions
            .iter()
            .filter_map(|a| map_action(a).map(str::to_string))
            .collect();
        out.push(HelperGoodInfo {
            goods_name: sanitize_name(&raw.name, &goods_id),
            status: infer_status(&actions, tab, &raw_text),
            goods_id,
            raw_text,
            available_actions: actions,
            source_tab: tab.key().to_string(),
        });
    }
    Ok(out)
}

/// 把 helpers + 动作包进单次 `evaluate` 的 IIFE（见模块文档的顶层声明约束）。
fn helper_expr(body: &str) -> String {
    // 仅格式串的括号需要转义；helpers/动作原文按字面替换。
    format!(
        "(function(){{ {helpers} {body} }})()",
        helpers = HELPER_GOODS_DOM_HELPERS,
        body = body
    )
}

fn read_expr(tab: HelperGoodTab) -> String {
    let tab = serde_json::to_string(tab.key()).expect("serializing a tab key cannot fail");
    helper_expr(&format!("return HelperGoods.read({tab});"))
}

fn switch_expr(tab: HelperGoodTab) -> String {
    let tab = serde_json::to_string(tab.key()).expect("serializing a tab key cannot fail");
    helper_expr(&format!("return HelperGoods.switchTab({tab});"))
}

fn act_expr(goods_id: &str, action: &str) -> String {
    let goods_id =
        serde_json::to_string(goods_id).expect("serializing a goods id cannot fail");
    let action = serde_json::to_string(action).expect("serializing an action cannot fail");
    helper_expr(&format!("return HelperGoods.act({goods_id}, {action});"))
}

fn page_url_of(value: &serde_json::Value) -> &str {
    value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
}

impl TauriBrowserDriver {
    /// 在已绑定的 `target` 上取跟播助手页租约，并校验当前 URL 仍在助手页。
    /// `session` 由调用方持有，租约生命周期挂靠其上。
    async fn helper_lease<'a>(
        session: &'a std::sync::Arc<cdp_driver::session::BrowserSession>,
        target_id: &str,
        cancel: TaskCancel,
    ) -> Result<TaskPage<'a>> {
        let mut page = session
            .task_page(target_id, cancel, LOCK_WAIT)
            .await
            .map_err(task_err)?;
        let url = page
            .evaluate("location.href", READ_WAIT)
            .await
            .map_err(task_err)?;
        let url = url.as_str().unwrap_or("");
        if !helper_page_url(url) {
            return Err(shop_err("当前页面不是跟播助手页（zs.kwaixiaodian.com/page/helper）"));
        }
        Ok(page)
    }

    /// 在现有页面中定位跟播助手页（供脚本化上车调用；不负责导航——进入助手页
    /// 是 `ensure_auth` 流程的职责，缺页时返回可操作错误而非抢占用户 Tab）。
    async fn find_helper_target(
        &self,
        profile_id: &str,
    ) -> Result<(std::sync::Arc<cdp_driver::session::BrowserSession>, String)> {
        let session = self.require_session(profile_id).await?;
        let pages = session
            .browser
            .pages()
            .await
            .map_err(|e| MultizenError::Cdp(format!("列出浏览器页面失败：{e}")))?;
        for page in pages.iter().take(MAX_PAGES_SCANNED) {
            let url = page
                .url()
                .await
                .map_err(|e| MultizenError::Cdp(format!("读取页面地址失败：{e}")))?;
            if url.as_deref().is_some_and(helper_page_url) {
                return Ok((session, page.target_id().as_ref().to_owned()));
            }
        }
        Err(shop_err(format!(
            "未找到跟播助手页，请先进入 {HELPER_PAGE_URL}（ensure_auth）",
        )))
    }

    async fn read_tab(
        page: &mut TaskPage<'_>,
        tab: HelperGoodTab,
    ) -> Result<Vec<HelperGoodInfo>> {
        let value = page
            .evaluate(&read_expr(tab), READ_WAIT)
            .await
            .map_err(task_err)?;
        decode_goods_list(value, tab).map_err(shop_err)
    }

    /// 读取指定产品页面的某个 Tab 商品列表。
    pub async fn shop_helper_read_goods(
        &self,
        profile_id: &str,
        target_id: &str,
        tab: &str,
        cancel: TaskCancel,
    ) -> Result<Vec<HelperGoodInfo>> {
        let tab = HelperGoodTab::parse(tab).map_err(shop_err)?;
        let session = self.require_session(profile_id).await?;
        let mut page = Self::helper_lease(&session, target_id, cancel).await?;
        Self::read_tab(&mut page, tab).await
    }

    /// 切换小黄车/待上车 Tab，等待列表同步后重读返回。
    pub async fn shop_helper_switch_tab(
        &self,
        profile_id: &str,
        target_id: &str,
        tab: &str,
        cancel: TaskCancel,
    ) -> Result<Vec<HelperGoodInfo>> {
        let tab = HelperGoodTab::parse(tab).map_err(shop_err)?;
        let session = self.require_session(profile_id).await?;
        let mut page = Self::helper_lease(&session, target_id, cancel.clone()).await?;
        let switched: serde_json::Value = page
            .evaluate(&switch_expr(tab), WRITE_WAIT)
            .await
            .map_err(task_err)?;
        if !helper_page_url(page_url_of(&switched)) {
            return Err(shop_err("切换期间页面已离开跟播助手页"));
        }
        if switched
            .get("clicked")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(shop_err("未找到对应 Tab 入口，助手页结构可能已变化（需真号校准）"));
        }
        // Tab 切换后列表异步刷新：有界轮询等第一批商品出现。
        let deadline =
            tokio::time::Instant::now() + TAB_SYNC_TIMEOUT;
        loop {
            let goods = Self::read_tab(&mut page, tab).await?;
            if !goods.is_empty() || tokio::time::Instant::now() >= deadline || cancel.is_cancelled()
            {
                return Ok(goods);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn click_goods(
        &self,
        profile_id: &str,
        target_id: &str,
        goods_id: &str,
        action: &str,
        cancel: TaskCancel,
    ) -> Result<HelperGoodActionResult> {
        if !valid_goods_id(goods_id) {
            return Err(shop_err("商品 ID 非法（仅数字，1–32 位）"));
        }
        let tab = if action == "on" {
            HelperGoodTab::ToAdd
        } else {
            HelperGoodTab::InCart
        };
        let session = self.require_session(profile_id).await?;
        let mut page = Self::helper_lease(&session, target_id, cancel).await?;
        let before = Self::read_tab(&mut page, tab).await?;
        if !before.iter().any(|g| g.goods_id == goods_id) {
            return Err(shop_err(format!(
                "商品 {goods_id} 不在{}列表中",
                if action == "on" { "待上车" } else { "小黄车" }
            )));
        }
        let clicked: serde_json::Value = page
            .evaluate(&act_expr(goods_id, action), WRITE_WAIT)
            .await
            .map_err(task_err)?;
        if !helper_page_url(page_url_of(&clicked)) {
            return Err(shop_err("操作期间页面已离开跟播助手页，点击是否生效未知"));
        }
        if clicked
            .get("clicked")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        {
            return Err(shop_err(format!(
                "未找到商品 {goods_id} 的{}按钮",
                if action == "on" { "上车" } else { "下车" }
            )));
        }
        // 点击后重读验证状态翻转：出现反向动作，或商品离开本 Tab，均视为生效。
        tokio::time::sleep(POST_CLICK_SETTLE).await;
        let after = Self::read_tab(&mut page, tab).await?;
        let flipped = match after.iter().find(|g| g.goods_id == goods_id) {
            None => true,
            Some(current) => {
                let want = if action == "on" { "下车" } else { "上车" };
                current.available_actions.iter().any(|a| a == want)
            }
        };
        let verb = if action == "on" { "上车" } else { "下车" };
        Ok(HelperGoodActionResult {
            detail: if flipped {
                format!("商品 {goods_id}{verb}已执行并验证")
            } else {
                format!("商品 {goods_id}{verb}点击疑似未生效，请人工核对")
            },
            ok: flipped,
            goods_id: goods_id.to_string(),
            action: action.to_string(),
            goods: after,
        })
    }

    /// 上车（小黄车加商品）。真号写动作，验收单独确认。
    pub async fn shop_helper_add_to_cart(
        &self,
        profile_id: &str,
        target_id: &str,
        goods_id: &str,
        cancel: TaskCancel,
    ) -> Result<HelperGoodActionResult> {
        self.click_goods(profile_id, target_id, goods_id, "on", cancel)
            .await
    }

    /// 下车（从小黄车移除）。
    pub async fn shop_helper_remove_from_cart(
        &self,
        profile_id: &str,
        target_id: &str,
        goods_id: &str,
        cancel: TaskCancel,
    ) -> Result<HelperGoodActionResult> {
        self.click_goods(profile_id, target_id, goods_id, "off", cancel)
            .await
    }

    /// 脚本化接口（供 shop-product-script 调用）：按 `profile_id` 找到助手页，
    /// 切待上车 Tab 后上车；商品已在车内则直接返回成功。
    pub async fn goods_on_shelf(
        &self,
        profile_id: &str,
        goods_id: &str,
        cancel: TaskCancel,
    ) -> Result<HelperGoodActionResult> {
        if !valid_goods_id(goods_id) {
            return Err(shop_err("商品 ID 非法（仅数字，1–32 位）"));
        }
        let (_session, target) = self.find_helper_target(profile_id).await?;
        // 已在车内：幂等成功，不重复点击。
        let in_cart = self
            .shop_helper_read_goods(profile_id, &target, "inCart", cancel.clone())
            .await?;
        if in_cart.iter().any(|g| g.goods_id == goods_id) {
            return Ok(HelperGoodActionResult {
                ok: true,
                goods_id: goods_id.to_string(),
                action: "on".to_string(),
                detail: format!("商品 {goods_id}已在车内，无需重复上车"),
                goods: in_cart,
            });
        }
        self.shop_helper_add_to_cart(profile_id, &target, goods_id, cancel)
            .await
    }

    /// 脚本化接口（供 shop-product-script 调用）：按 `profile_id` 找到助手页下车；
    /// 商品已不在车内则幂等成功。
    pub async fn goods_off_shelf(
        &self,
        profile_id: &str,
        goods_id: &str,
        cancel: TaskCancel,
    ) -> Result<HelperGoodActionResult> {
        if !valid_goods_id(goods_id) {
            return Err(shop_err("商品 ID 非法（仅数字，1–32 位）"));
        }
        let (_session, target) = self.find_helper_target(profile_id).await?;
        let in_cart = self
            .shop_helper_read_goods(profile_id, &target, "inCart", cancel.clone())
            .await?;
        if !in_cart.iter().any(|g| g.goods_id == goods_id) {
            return Ok(HelperGoodActionResult {
                ok: true,
                goods_id: goods_id.to_string(),
                action: "off".to_string(),
                detail: format!("商品 {goods_id}已不在车内，无需下车"),
                goods: in_cart,
            });
        }
        self.shop_helper_remove_from_cart(profile_id, &target, goods_id, cancel)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tab_parses_camel_and_snake_case() {
        assert_eq!(HelperGoodTab::parse("inCart"), Ok(HelperGoodTab::InCart));
        assert_eq!(HelperGoodTab::parse("in_cart"), Ok(HelperGoodTab::InCart));
        assert_eq!(HelperGoodTab::parse(" Cart "), Ok(HelperGoodTab::InCart));
        assert_eq!(HelperGoodTab::parse("toAdd"), Ok(HelperGoodTab::ToAdd));
        assert_eq!(HelperGoodTab::parse("TO_ADD"), Ok(HelperGoodTab::ToAdd));
        assert!(HelperGoodTab::parse("cart2").is_err());
        assert!(HelperGoodTab::parse("").is_err());
        assert_eq!(HelperGoodTab::InCart.key(), "inCart");
        assert_eq!(HelperGoodTab::ToAdd.key(), "toAdd");
    }

    #[test]
    fn helper_url_matches_exact_origin_and_path() {
        assert!(helper_page_url("https://zs.kwaixiaodian.com/page/helper"));
        assert!(helper_page_url(
            "https://zs.kwaixiaodian.com:443/page/helper?from=live"
        ));
        assert!(helper_page_url("https://zs.kwaixiaodian.com/page/helper/"));
        for url in [
            "http://zs.kwaixiaodian.com/page/helper",
            "https://zs.kwaixiaodian.com.evil.test/page/helper",
            "https://evil.test/zs.kwaixiaodian.com/page/helper",
            "https://zs.kwaixiaodian.com@evil.test/page/helper",
            "https://u@zs.kwaixiaodian.com/page/helper",
            "https://zs.kwaixiaodian.com:444/page/helper",
            // CPS 是另一个源：必须被本守卫拒绝，反之亦然。
            "https://cps.kwaixiaodian.com/page/helper",
            "https://s.kwaixiaodian.com/zone/home",
            "https://zs.kwaixiaodian.com/page/other",
            "https://zs.kwaixiaodian.com/",
        ] {
            assert!(!helper_page_url(url), "{url}");
        }
    }

    #[test]
    fn goods_id_is_numeric_and_bounded() {
        assert!(valid_goods_id("12345"));
        assert!(valid_goods_id("0"));
        assert!(valid_goods_id(&"9".repeat(32)));
        for id in ["", "1234a", "12 34", "ID123", "123\n", &"9".repeat(33)] {
            assert!(!valid_goods_id(id), "{id}");
        }
    }

    fn decoded_goods(items: serde_json::Value, tab: HelperGoodTab) -> Vec<HelperGoodInfo> {
        decode_goods_list(
            json!({"url": "https://zs.kwaixiaodian.com/page/helper", "goods": items}),
            tab,
        )
        .unwrap()
    }

    #[test]
    fn decode_maps_actions_and_infers_status() {
        let goods = decoded_goods(
            json!([
                {"id": "1001", "name": "红苹果", "rawText": "红苹果 上车", "actions": ["on"]},
                {"id": "1002", "name": "绿香蕉", "rawText": "绿香蕉 下车", "actions": ["off"]},
                {"id": "1003", "name": "黄梨", "rawText": "黄梨", "actions": []},
            ]),
            HelperGoodTab::ToAdd,
        );
        assert_eq!(goods.len(), 3);
        assert_eq!(goods[0].available_actions, vec!["上车"]);
        assert_eq!(goods[0].status, HelperGoodStatus::Available);
        assert_eq!(goods[0].source_tab, "toAdd");
        assert_eq!(goods[1].status, HelperGoodStatus::OnShelf);
        assert_eq!(goods[2].status, HelperGoodStatus::Unknown);
        // 同一动作在 InCart 默认已上车。
        let in_cart = decoded_goods(
            json!([{"id": "1001", "name": "红苹果", "rawText": "x", "actions": ["on"]}]),
            HelperGoodTab::InCart,
        );
        assert_eq!(in_cart[0].status, HelperGoodStatus::OnShelf);
    }

    #[test]
    fn decode_prefers_explicit_status_text() {
        let goods = decoded_goods(
            json!([
                {"id": "2001", "name": "A", "rawText": "A 已上车", "actions": ["on"]},
                {"id": "2002", "name": "B", "rawText": "B 已下架", "actions": ["off"]},
            ]),
            HelperGoodTab::ToAdd,
        );
        assert_eq!(goods[0].status, HelperGoodStatus::OnShelf);
        assert_eq!(goods[1].status, HelperGoodStatus::OffShelf);
    }

    #[test]
    fn decode_rejects_bad_ids_and_off_page_shapes() {
        // 非法 ID 整批拒绝，不静默丢弃。
        for id in ["abc", "", "12-34"] {
            assert!(
                decode_goods_list(
                    json!({"url": HELPER_PAGE_URL, "goods": [{"id": id}]}),
                    HelperGoodTab::ToAdd,
                )
                .is_err(),
                "{id}"
            );
        }
        // 离开助手页 / 结构破坏同样拒绝。
        assert!(decode_goods_list(
            json!({"url": "https://cps.kwaixiaodian.com/", "goods": []}),
            HelperGoodTab::ToAdd,
        )
        .is_err());
        assert!(
            decode_goods_list(json!({"url": HELPER_PAGE_URL}), HelperGoodTab::ToAdd).is_err()
        );
        // 未知动作被过滤而非失败。
        let goods = decoded_goods(
            json!([{"id": "3001", "name": "C", "rawText": "C", "actions": ["on", "explain"]}]),
            HelperGoodTab::ToAdd,
        );
        assert_eq!(goods[0].available_actions, vec!["上车"]);
    }

    #[test]
    fn decode_sanitizes_names_and_noise() {
        let goods = decoded_goods(
            json!([
                {"id": "4001", "name": "加载中", "rawText": "t", "actions": []},
                {"id": "4002", "name": "", "rawText": "t", "actions": []},
                {"id": "4003", "name": "好 货", "rawText": "t", "actions": []},
            ]),
            HelperGoodTab::ToAdd,
        );
        // 噪声名/空名回退到商品 ID，保证前端总有可显示文本。
        assert_eq!(goods[0].goods_name, "4001");
        assert_eq!(goods[1].goods_name, "4002");
        assert_eq!(goods[2].goods_name, "好 货");
    }

    #[test]
    fn dom_helpers_contain_all_required_strategies() {
        let js = HELPER_GOODS_DOM_HELPERS;
        for marker in [
            "helperNormalizeText",
            "helperIsVisible",
            "helperReadId",
            "HELPER_NOISE_TEXTS",
            "helperIsNoiseText",
            "HelperGoods",
            // readId 五策略：文本 / id 属性 / input value / data 属性 / 嵌套属性。
            "ID\\s*",
            "goods[-_]?",
            "input-value",
            "data-attr",
            "data-goods-id",
            "nested-href",
        ] {
            assert!(js.contains(marker), "missing {marker}");
        }
        // Rust 噪声表与 JS 同源：每一项都必须出现在 JS 常量中。
        for noise in HELPER_NOISE_TEXTS {
            assert!(js.contains(noise), "noise {noise} missing from JS");
        }
    }

    #[test]
    fn action_expressions_wrap_helpers_in_one_iife() {
        for expr in [
            read_expr(HelperGoodTab::ToAdd),
            switch_expr(HelperGoodTab::InCart),
            act_expr("12345", "on"),
        ] {
            assert!(expr.starts_with("(function(){"));
            assert!(expr.ends_with("})()"));
            assert!(expr.contains(HELPER_GOODS_DOM_HELPERS));
            // JSON 字符串即 JS 字面量：ID/动作已正确转义嵌入。
            assert!(!expr.contains("undefined"));
        }
        assert!(read_expr(HelperGoodTab::ToAdd).contains("HelperGoods.read(\"toAdd\")"));
        assert!(act_expr("12345", "off").contains("HelperGoods.act(\"12345\", \"off\")"));
    }

    #[test]
    fn commands_are_registered_in_actual_tauri_handler() {
        let source = include_str!("../lib.rs");
        let handler = source
            .split(".invoke_handler(tauri::generate_handler![")
            .nth(1)
            .unwrap()
            .split("])")
            .next()
            .unwrap();
        for command in [
            "shop_helper_read_goods",
            "shop_helper_switch_tab",
            "shop_helper_add_to_cart",
            "shop_helper_remove_from_cart",
        ] {
            assert!(
                handler
                    .lines()
                    .any(|line| line.trim() == format!("{command},")),
                "{command}"
            );
        }
        let adapter = include_str!("../commands/shop_helper.rs");
        assert_eq!(adapter.matches("#[tauri::command]").count(), 4);
    }
}
