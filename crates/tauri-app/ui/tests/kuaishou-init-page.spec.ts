import { readFileSync } from "node:fs";
import { expect, test, type Page } from "@playwright/test";

// The actual production adapter, executed only against inert synthetic local DOM.
const script = readFileSync(new URL("../../src/driver/account_init/page.js", import.meta.url), "utf8");
const shop = "https://s.kwaixiaodian.com";
const sliceUrl = `${shop}/zone/short-video-b/slice`;
const subjectUrl = `${shop}/zone/shop/info/qualification`;
const labels = ["直播中商品详解切片权限托管", "直播爆品切片返场权限托管", "直播引流片段发布权限托管", "切片个人主页展示位置"];
async function run(page: Page, action: string, argument: unknown = null, url = sliceUrl, actualId = "12345") {
  return page.evaluate(({ script, action, argument, url, actualId }) => {
    const location = new URL(url);
    const adapter = new Function("location", `return (${script})`)(location);
    try { return { ok: true, value: adapter(action, "12345", argument, () => ({ platformUserId: actualId })).result }; }
    catch { return { ok: false, value: null }; }
  }, { script, action, argument, url, actualId });
}

async function sliceFixture(page: Page) {
  await page.setContent(`<div class="kwaishop-tianhe-shortVideoB-pc-drawer-content">
    <h2 class="kwaishop-tianhe-shortVideoB-pc-drawer-title">直播切片托管设置</h2>
    <section class="WuFhZjfBowihPEVt5UUF"><div class="permission-count-header"><h3 class="gh6LEC5pPxFLJTnYMjOy">全自动发布权限</h3>
      <p id="count">开启4/4</p></div>
      ${labels.map(label => `<div class="collapseHeader"><label><input type="checkbox" checked>${label}</label></div>`).join("")}
      <label><input id="limit" type="checkbox">限制单位时间切片发布量</label>
      <label><input id="radio" type="radio" checked>不设私</label><input id="number" type="number" value="3">
    </section>
    <section><h3>智能生产权限</h3><button role="switch" aria-checked="true">智能生产</button></section>
    <button aria-label="Close">关闭抽屉</button>
  </div>`);
  await page.evaluate(() => {
    document.querySelectorAll('.collapseHeader input').forEach(box => box.addEventListener("change", () => {
      const count = document.querySelectorAll('.collapseHeader input:checked').length;
      document.querySelector("#count")!.textContent = count ? `开启${count}/4` : '未开启';
    }));
  });
}

test.beforeEach(async ({ page }) => {
  // Even a future accidental src/fetch addition must not reach any remote endpoint.
  await page.route(/https?:\/\//, route => route.abort());
});

test("observed drawer header icon closes only its titled drawer and rejects ambiguous controls", async ({ page }) => {
  async function headerIcon(duplicate = false) {
    await sliceFixture(page);
    await page.evaluate(duplicate => {
      const drawer = document.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-content')!;
      drawer.querySelector('button[aria-label="Close"]')!.remove();
      const header = document.createElement('div'); header.className = 'kwaishop-tianhe-shortVideoB-pc-drawer-header';
      header.append(drawer.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-title')!);
      const extra = document.createElement('div'); extra.className = 'kwaishop-tianhe-shortVideoB-pc-drawer-extra';
      extra.innerHTML = '<span class="anticon anticon-system-close-medium-line" role="img">×</span>';
      if (duplicate) extra.append(extra.firstElementChild!.cloneNode(true));
      extra.querySelectorAll('span').forEach(icon => icon.addEventListener('click', () => { drawer.setAttribute('data-closed', 'true'); }));
      header.append(extra); drawer.prepend(header);
      document.body.insertAdjacentHTML('beforeend', '<span class="anticon anticon-system-close-medium-line" role="img" id="outside-close">×</span>');
      document.querySelector('#outside-close')!.addEventListener('click', () => document.body.setAttribute('data-wrong-close', 'true'));
    }, duplicate);
  }
  await headerIcon();
  expect((await run(page, "close")).ok).toBe(true);
  await expect(page.locator('.kwaishop-tianhe-shortVideoB-pc-drawer-content')).toHaveAttribute('data-closed', 'true');
  await expect(page.locator('body')).not.toHaveAttribute('data-wrong-close', 'true');
  await headerIcon(true);
  expect((await run(page, "close")).ok).toBe(false);
  await expect(page.locator('.kwaishop-tianhe-shortVideoB-pc-drawer-content')).not.toHaveAttribute('data-closed', 'true');
  await headerIcon();
  await page.locator('.kwaishop-tianhe-shortVideoB-pc-drawer-extra').evaluate(extra => extra.setAttribute('aria-disabled', 'true'));
  expect((await run(page, "close")).ok).toBe(false);
  await headerIcon();
  await page.locator('.kwaishop-tianhe-shortVideoB-pc-drawer-content').evaluate(drawer => drawer.insertAdjacentHTML('beforeend', '<button aria-label="Close">×</button>'));
  expect((await run(page, "close")).ok).toBe(false); // visible button plus icon is ambiguous
});

test("entry readiness is read-only and never treats duplicate settings entries as ready", async ({ page }) => {
  await page.setContent('<div class="js-page-content"></div>');
  expect(await run(page, "open-ready")).toEqual({ ok: true, value: false });
  await page.evaluate(() => {
    const button = document.createElement('button'); button.textContent = '修改设置';
    button.onclick = () => document.body.setAttribute('data-opened', 'true');
    document.querySelector('.js-page-content')!.append(button);
  });
  expect(await run(page, "open-ready")).toEqual({ ok: true, value: true });
  await expect(page.locator('body')).not.toHaveAttribute('data-opened', 'true');
  expect((await run(page, "open")).ok).toBe(true);
  await expect(page.locator('body')).toHaveAttribute('data-opened', 'true');
  await page.locator('.js-page-content button').evaluate(button => { button.textContent = '去设置'; });
  expect(await run(page, "open-ready")).toEqual({ ok: true, value: true });
  await page.evaluate(() => document.querySelector('.js-page-content')!.append(document.querySelector('button')!.cloneNode(true)));
  expect((await run(page, "open-ready")).ok).toBe(false);
  await page.locator('.js-page-content button').first().evaluate(button => { button.textContent = '修改设置'; });
  expect((await run(page, "open-ready")).ok).toBe(false); // the union is ambiguous too
});

test("zero permission state uses the exact title header, never loose section text", async ({ page }) => {
  await sliceFixture(page);
  await page.locator('.collapseHeader input').evaluateAll(boxes => boxes.forEach(box => { (box as HTMLInputElement).checked = false; }));
  await page.locator('#count').evaluate(count => { count.textContent = '未开启'; });
  expect(await run(page, 'slice')).toEqual({ ok: true, value: [true, true, true, true] });
  await page.locator('.collapseHeader input').first().evaluate(box => { (box as HTMLInputElement).checked = true; });
  expect((await run(page, 'slice')).ok).toBe(false); // zero marker cannot conceal an enabled permission
  await page.locator('.collapseHeader input').first().evaluate(box => { (box as HTMLInputElement).checked = false; });
  for (const invalid of ['', '开启0/4', '未开启 未知', '未开启 开启1/4']) {
    await page.locator('#count').evaluate((count, invalid) => { count.textContent = invalid; }, invalid);
    expect((await run(page, 'slice')).ok, invalid).toBe(false);
  }
  await page.locator('#count').evaluate(count => { count.textContent = '未知'; });
  await page.locator('section.WuFhZjfBowihPEVt5UUF').evaluate(section => section.insertAdjacentHTML('beforeend', '<p>未开启 开启4/4</p>'));
  expect((await run(page, 'slice')).ok).toBe(false); // unrelated child text is not a count header
  await page.locator('#count').evaluate(count => { count.textContent = '开启3/4'; });
  await page.locator('.collapseHeader input').evaluateAll(boxes => boxes.forEach((box, i) => { (box as HTMLInputElement).checked = i < 3; }));
  expect(await run(page, 'slice')).toEqual({ ok: true, value: [false, false, false, true] });
  await page.locator('#count').evaluate(count => { count.textContent = '开启2/4'; });
  expect((await run(page, 'slice')).ok).toBe(false);
  await page.locator('#count').evaluate(count => { count.textContent = '开启3/4'; count.parentElement!.append(count.cloneNode(true)); });
  expect((await run(page, 'slice')).ok).toBe(false); // ambiguous header values
});

test("all four exact main permissions are idempotently disabled; other controls stay untouched", async ({ page }) => {
  await sliceFixture(page);
  expect(await run(page, "slice")).toEqual({ ok: true, value: [false, false, false, false] });
  for (let index = 0; index < 4; index++) {
    expect((await run(page, "off", index)).ok).toBe(true);
    expect((await run(page, "off", index)).ok).toBe(true); // Never toggles back on.
  }
  expect(await run(page, "slice")).toEqual({ ok: true, value: [true, true, true, true] });
  await expect(page.locator("#limit")).not.toBeChecked();
  await expect(page.locator("#radio")).toBeChecked();
  await expect(page.locator("#number")).toHaveValue("3");
  await expect(page.getByRole("switch")).toHaveAttribute("aria-checked", "true");
});

test("route, account, unexpected main items, disabled items and visible confirmations fail before mutation", async ({ page }) => {
  for (const url of [`${shop}/zone/home`, "https://s.kwaixiaodian.com.evil.invalid/zone/short-video-b/slice", "https://user@s.kwaixiaodian.com/zone/short-video-b/slice"]) {
    await sliceFixture(page);
    expect((await run(page, "off", 0, url)).ok).toBe(false);
    await expect(page.locator('.collapseHeader input:checked')).toHaveCount(4);
  }
  await sliceFixture(page);
  expect((await run(page, "off", 0, sliceUrl, "99999")).ok).toBe(false);
  for (const fault of ["extra", "disabled", "count", "modal"]) {
    await sliceFixture(page);
    await page.evaluate(fault => {
      if (fault === "extra") document.querySelector("section")!.insertAdjacentHTML("beforeend", '<div class="collapseHeader"><label><input type="checkbox" checked>新增未知权限</label></div>');
      if (fault === "disabled") (document.querySelector('.collapseHeader input') as HTMLInputElement).disabled = true;
      if (fault === "count") document.querySelector("#count")!.textContent = "开启3/4";
      if (fault === "modal") document.body.insertAdjacentHTML("beforeend", '<div role="dialog"><button>确定</button></div>');
    }, fault);
    expect((await run(page, "off", 0)).ok, fault).toBe(false);
    await expect(page.locator('.collapseHeader input').first()).toBeChecked();
  }
});

test("subject adapter reads only the selected active panel and field-local loaded pixels", async ({ page }) => {
  await page.setContent(`<button role="tab" aria-selected="false" aria-controls="main">主体信息</button>
    <button role="tab" aria-selected="true" aria-controls="talent">达人主体信息</button>
    <div id="main"><div class="section-item"><span class="item-title">经营者姓名</span><span class="item-content">不能读取的旧面板</span></div></div>
    <div id="talent" class="ant-tabs-tabpane-active">
      ${[["分销者姓名", "合*本"], ["证件类型", "大陆居民身份证"], ["证件号", "990001************"], ["证件照片", '<img id="photo">']].map(([label, value]) => `<div class="ant-row-flex"><span class="ant-col">${label}</span><span class="ant-col">${value}</span></div>`).join("")}
      <img id="unrelated">
    </div>`);
  await page.evaluate(async shop => {
    const canvas = document.createElement("canvas"); canvas.width = 16; canvas.height = 16;
    canvas.getContext("2d")!.fillRect(0, 0, 16, 16);
    const img = document.querySelector("#photo") as HTMLImageElement;
    img.src = canvas.toDataURL("image/png"); await img.decode();
    // Source getter is synthetic only; no same-origin or platform image is requested.
    Object.defineProperty(img, "currentSrc", { configurable: true, value: `${shop}/synthetic.png` });
  }, shop);
  const result = await run(page, "subject", "达人主体信息", subjectUrl);
  expect(result.ok).toBe(true);
  expect(result.value).toMatchObject({ name: "合*本", card: "990001************" });
  expect(result.value.pictures).toHaveLength(1);
  expect((await run(page, "subject", "主体信息", subjectUrl)).ok).toBe(false);
  expect((await run(page, "subject", "未知Tab", subjectUrl)).ok).toBe(false);
  await page.locator("#photo").evaluate(img => Object.defineProperty(img, "currentSrc", { configurable: true, value: "https://example.invalid/protected.png" }));
  expect((await run(page, "subject", "达人主体信息", subjectUrl)).ok).toBe(false);
});
