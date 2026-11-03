/** Shared input modality and native command cancellation for application surfaces. */
export function installInputInteractions(root: HTMLElement) {
  const document = root.ownerDocument;
  const window = document.defaultView;
  const controller = new AbortController();
  const options = { capture: true, signal: controller.signal };
  let active: { button: HTMLButtonElement; pointer?: number; key?: string } | null = null;
  let cancelledSpace = false;
  const blocked = new WeakSet<HTMLButtonElement>();
  document.documentElement.dataset.inputMode = 'pointer';
  function button(target: EventTarget | null) {
    const candidate = target instanceof Element ? target.closest('button:not(.drag-handle)') : null;
    return candidate instanceof HTMLButtonElement && root.contains(candidate) ? candidate : null;
  }
  function finish(cancelled = false) {
    if (!active) return;
    active.button.dataset.down = 'false';
    if (cancelled && active.pointer !== undefined) blocked.add(active.button);
    if (cancelled && active.key === ' ') cancelledSpace = true;
    active = null;
  }
  function inside(element: HTMLElement, event: PointerEvent) {
    const bounds = element.getBoundingClientRect();
    return event.clientX >= bounds.left && event.clientX < bounds.right && event.clientY >= bounds.top && event.clientY < bounds.bottom;
  }
  document.addEventListener('pointerdown', (event) => {
    document.documentElement.dataset.inputMode = 'pointer';
    finish(true);
    const target = button(event.target);
    if (!target || target.disabled || event.button !== 0) return;
    blocked.delete(target);
    active = { button: target, pointer: event.pointerId };
    target.dataset.down = 'true';
  }, options);
  document.addEventListener('pointermove', (event) => {
    if (!active || active.pointer !== event.pointerId) return;
    if (!(event.buttons & 1)) { finish(true); return; }
    active.button.dataset.down = String(inside(active.button, event));
  }, options);
  document.addEventListener('pointerup', (event) => {
    if (active?.pointer === event.pointerId) finish(!inside(active.button, event));
  }, options);
  const cancelPointer = (event: PointerEvent) => { if (active?.pointer === event.pointerId) finish(true); };
  document.addEventListener('pointercancel', cancelPointer, options);
  document.addEventListener('lostpointercapture', cancelPointer, options);
  document.addEventListener('keydown', (event) => {
    if (event.isComposing) return;
    const editable = event.target instanceof Element && event.target.closest('input,textarea,[contenteditable="true"]');
    if (event.key === 'Tab' || (!editable && !event.ctrlKey && !event.metaKey && !event.altKey)) document.documentElement.dataset.inputMode = 'keyboard';
  }, options);
  document.addEventListener('keydown', (event) => {
    if (event.isComposing || event.defaultPrevented) return;
    if (event.key === 'Escape' && active) { finish(true); event.preventDefault(); return; }
    if (!['Enter', ' '].includes(event.key)) return;
    const target = button(event.target);
    if (!target) return;
    if (target.disabled || event.repeat) { event.preventDefault(); return; }
    finish(); cancelledSpace = false;
    active = { button: target, key: event.key };
    target.dataset.down = 'true';
  }, { signal: controller.signal });
  document.addEventListener('keyup', (event) => {
    if (event.key === ' ' && cancelledSpace) { event.preventDefault(); cancelledSpace = false; }
    if (active?.key === event.key) finish();
  }, options);
  document.addEventListener('focusout', (event) => { if (active?.button === event.target) finish(true); }, options);
  document.addEventListener('click', (event) => {
    const target = button(event.target);
    if (target && (target.disabled || (event.detail > 0 && blocked.has(target)))) {
      blocked.delete(target); event.preventDefault(); event.stopImmediatePropagation();
    }
  }, options);
  document.addEventListener('visibilitychange', () => { if (document.hidden) finish(true); }, options);
  window?.addEventListener('blur', () => finish(true), { signal: controller.signal });
  return () => { finish(true); controller.abort(); };
}
