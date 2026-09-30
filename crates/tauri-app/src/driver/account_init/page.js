// Fixed main-frame adapter. No network/storage APIs and no arbitrary selectors from IPC.
(action, expectedId, argument, readIdentity) => {
  const fail = () => { throw new Error('account-init-page-unsupported'); };
  const identity = readIdentity();
  if (window !== window.top || identity.platformUserId !== expectedId || location.origin !== 'https://s.kwaixiaodian.com') fail();
  const visible = e => !!e && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden' && getComputedStyle(e).display !== 'none';
  const text = e => (e?.textContent || '').trim();
  const one = xs => { if (xs.length !== 1) fail(); return xs[0]; };
  const dialogs = [...document.querySelectorAll('[role="dialog"],.ant-modal,.kwaishop-tianhe-shortVideoB-pc-modal')].filter(visible);
  if (dialogs.length) fail(); // Unknown confirmation never clicked.
  let result = null;
  if (action === 'identity') result = true;
  else if (action === 'tab' || action === 'subject') {
    const tab = one([...document.querySelectorAll('[role="tab"]')].filter(e => text(e) === argument && visible(e)));
    if (action === 'tab') { if (tab.getAttribute('aria-selected') !== 'true') tab.click(); result = true; }
    else {
      if (tab.getAttribute('aria-selected') !== 'true') fail();
      const panel = one([...document.querySelectorAll('.ant-tabs-tabpane-active')].filter(visible));
      // aria-controls association is checked when the platform provides it.
      if (tab.getAttribute('aria-controls') && panel.id !== tab.getAttribute('aria-controls')) fail();
      const main = argument === '主体信息';
      const value = label => {
        if (main) {
          const row = one([...panel.querySelectorAll('.section-item')].filter(r => text(r.querySelector(':scope > .item-title')) === label));
          return one([...row.querySelectorAll(':scope > .item-content')]);
        }
        const row = one([...panel.querySelectorAll('.ant-row-flex')].filter(r => text(r.querySelector(':scope > .ant-col')) === label));
        const cols = [...row.querySelectorAll(':scope > .ant-col')];
        if (cols.length !== 2) fail(); return cols[1];
      };
      if (!/大陆.*居民.*身份证/.test(text(value('证件类型')))) fail();
      const name = text(value(main ? '经营者姓名' : '分销者姓名'));
      const card = text(value(main ? '经营者证件号码' : '证件号'));
      if (name.length > 100 || card.length > 64) fail();
      const field = value(main ? '经营者证件' : '证件照片');
      const images = [...field.querySelectorAll('img')];
      if (!images.length || images.length > 8) fail();
      const pictures = images.map(img => {
        const url = new URL(img.currentSrc);
        if (url.protocol !== 'https:' || url.origin !== location.origin || url.username || url.password || !img.complete || !img.naturalWidth || !img.naturalHeight || img.naturalWidth > 10000 || img.naturalHeight > 10000 || img.naturalWidth * img.naturalHeight > 16000000) fail();
        const canvas = document.createElement('canvas');
        canvas.width = img.naturalWidth; canvas.height = img.naturalHeight;
        const ctx = canvas.getContext('2d'); if (!ctx) fail();
        ctx.drawImage(img, 0, 0);
        const data = canvas.toDataURL('image/png');
        const prefix = 'data:image/png;base64,';
        if (!data.startsWith(prefix) || data.length > 13981040 + prefix.length) fail();
        return data.slice(prefix.length);
      });
      result = { name, card, pictures };
    }
  } else if (action === 'open') {
    const button = one([...document.querySelectorAll('.js-page-content button')].filter(e => text(e) === '修改设置' && visible(e) && !e.disabled));
    button.click(); result = true;
  } else {
    const drawer = one([...document.querySelectorAll('.kwaishop-tianhe-shortVideoB-pc-drawer-content')].filter(e => visible(e) && text(e.querySelector('.kwaishop-tianhe-shortVideoB-pc-drawer-title')) === '直播切片托管设置'));
    if (action === 'close') {
      // Only an explicit drawer-close control, never text "关闭" on a radio.
      const close = one([...drawer.querySelectorAll('button.kwaishop-tianhe-shortVideoB-pc-drawer-close,button[aria-label="Close"],button[aria-label="关闭"]')].filter(e => visible(e) && !e.disabled));
      close.click(); result = true;
    } else {
      const title = one([...drawer.querySelectorAll('.gh6LEC5pPxFLJTnYMjOy')].filter(e => text(e) === '全自动发布权限'));
      const section = title.closest('.WuFhZjfBowihPEVt5UUF'); if (!section) fail();
      const labels = ['直播中商品详解切片权限托管','直播爆品切片返场权限托管','直播引流片段发布权限托管','切片个人主页展示位置'];
      const headers = [...section.querySelectorAll('.collapseHeader label')].filter(e => e.querySelector('input[type="checkbox"]'));
      if (headers.length !== 4 || headers.some(e => !labels.includes(text(e)))) fail();
      const boxes = labels.map(label => one([...one(headers.filter(e => text(e) === label)).querySelectorAll('input[type="checkbox"]')]));
      if (boxes.some(e => e.disabled || e.indeterminate)) fail();
      const count = /开启\s*([0-4])\s*\/\s*4/.exec(text(section));
      if (!count || +count[1] !== boxes.filter(e => e.checked).length) fail();
      if (action === 'off') {
        if (!Number.isInteger(argument) || argument < 0 || argument > 3) fail();
        if (boxes[argument].checked) boxes[argument].click();
      } else if (action !== 'slice') fail();
      result = boxes.map(e => !e.checked);
    }
  }
  if (readIdentity().platformUserId !== expectedId || location.origin !== 'https://s.kwaixiaodian.com') fail();
  return { epoch: String(performance.timeOrigin), result };
}
