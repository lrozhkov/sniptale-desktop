export function Chevron({ expanded }: { expanded: boolean }) {
  return <svg className="chevron" viewBox="0 0 16 16" fill="none" aria-hidden="true" focusable="false">
    <path d={expanded ? 'M3 10 L8 6 L13 10' : 'M3 6 L8 10 L13 6'} stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
  </svg>;
}
