import { Check } from 'lucide-react';
import { Chevron } from './chevron';
import { useEffect, useId, useRef, useState } from 'react';
import { autoUpdate, flip, offset, shift, size, useFloating } from '@floating-ui/react-dom';
import type { ReactNode } from 'react';

export function Popover({ label, children, content, disabled = false, role = 'dialog', appearance = 'tool', chevron = true, description }: {
  label: string; children: ReactNode; content: (close: () => void) => ReactNode;
  description?: string; disabled?: boolean; role?: 'menu' | 'dialog'; appearance?: 'tool' | 'field'; chevron?: boolean;
}) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const { refs, floatingStyles, isPositioned } = useFloating({
    open, placement: 'bottom-end', whileElementsMounted: autoUpdate,
    middleware: [offset(10), flip({ padding: 8 }), shift({ padding: 8 }), size({ padding: 8,
      apply({ availableWidth, availableHeight, elements, rects }) {
        Object.assign(elements.floating.style, {
          maxWidth: `${Math.min(330, availableWidth)}px`, minWidth: `${Math.min(appearance === 'field' ? rects.reference.width : 190, availableWidth)}px`,
          maxHeight: `${Math.max(60, availableHeight)}px`, overflowY: 'auto',
        });
      },
    })],
  });
  useEffect(() => {
    if (!open) return;
    const selected = root.current?.querySelector<HTMLElement>('[aria-checked="true"]:not(:disabled), input:checked:not(:disabled)')
      ?? root.current?.querySelector<HTMLElement>('[data-floating-surface] button:not(:disabled), [data-floating-surface] input:not(:disabled)');
    if (isPositioned && document.documentElement.dataset.inputMode === 'keyboard') selected?.focus();
    const dismiss = (event: PointerEvent) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    const loseWindow = () => setOpen(false);
    document.addEventListener('pointerdown', dismiss);
    window.addEventListener('blur', loseWindow);
    return () => { document.removeEventListener('pointerdown', dismiss); window.removeEventListener('blur', loseWindow); };
  }, [open, isPositioned]);
  function close() {
    setOpen(false);
    if (refs.reference.current instanceof HTMLElement) refs.reference.current.focus();
  }
  return <div className="choice-menu" ref={root} onBlur={(event) => {
    // The trigger belongs to this focus scope. A second click must close, not reopen.
    if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }}>
    <button type="button" ref={refs.setReference} className={`tool-button menu-trigger ${appearance === 'field' ? 'popover-trigger--field' : ''}`} title={label} aria-label={label}
      aria-describedby={description ? `${id}-value` : undefined} aria-haspopup={role} aria-expanded={open} aria-controls={open ? id : undefined}
      data-toolbar-item="true" disabled={disabled} onClick={() => setOpen((current) => !current)}
      onKeyDown={(event) => { if (event.key === 'ArrowDown') {
        event.preventDefault(); setOpen(true);
        if (open) root.current?.querySelector<HTMLElement>('[data-floating-surface] button:not(:disabled), [data-floating-surface] input:not(:disabled)')?.focus();
      } }}>
      {children}{chevron && <Chevron expanded={open} />}
    </button>
    {description && <span hidden id={`${id}-value`}>{description}</span>}
    {open && <div className={role === 'menu' ? 'menu-surface' : 'parameter-surface'} role={role} id={id} ref={refs.setFloating}
      style={{ ...floatingStyles, visibility: isPositioned ? 'visible' : 'hidden' }}
      data-floating-surface="true" aria-label={label}
      onKeyDown={(event) => {
        if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); }
        if (role === 'menu' && ['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
          event.preventDefault(); event.stopPropagation();
          const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
          const current = items.indexOf(document.activeElement as HTMLButtonElement);
          const index = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 :
            (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
          items[index]?.focus();
        }
      }}>{content(close)}</div>}
  </div>;
}
export type MenuChoice = { value: string; label: string; icon?: ReactNode; disabled?: boolean; detail?: string };
export function ChoiceMenu({ label, value, choices, onChange, children, disabled = false, appearance = 'tool', chevron = true }: {
  label: string; value: string; choices: MenuChoice[]; onChange: (value: string) => void;
  children: ReactNode; disabled?: boolean; appearance?: 'tool' | 'field'; chevron?: boolean;
}) {
  return <Popover label={label} description={choices.find((choice) => choice.value === value)?.label} disabled={disabled} appearance={appearance} chevron={chevron} role="menu" content={(close) => <>
    {choices.map((choice) => <button type="button" role="menuitemradio" tabIndex={-1} aria-checked={value === choice.value}
      disabled={choice.disabled} key={choice.value} onClick={() => { onChange(choice.value); close(); }}>
      {choice.icon}<span>{choice.label}{choice.detail && <small>{choice.detail}</small>}</span>
      <Check className="choice-check" data-selected={value === choice.value} aria-hidden="true" />
    </button>)}
  </>}>{children}</Popover>;
}
