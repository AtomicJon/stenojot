import type { ReactNode } from 'react';
import s from './Panel.module.scss';

/** Props for the Panel component. */
interface PanelProps {
  /** Optional section title displayed as an uppercase label. */
  title?: string;
  /** Panel contents. */
  children: ReactNode;
  /** Additional CSS class names to apply to the panel. */
  className?: string;
}

/** Styled section container with optional title heading. */
export function Panel({ title, children, className }: PanelProps) {
  return (
    <section className={`${s.panel} ${className ?? ''}`}>
      {title && <h2 className={s.title}>{title}</h2>}
      {children}
    </section>
  );
}
