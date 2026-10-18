import type { HTMLAttributes } from 'react';
import { cn } from '@/lib/utils';

/** Original hexagonal voice waveform mark, following the selected accent. */
export function BrandMark({ className, ...props }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      aria-hidden="true"
      data-brand-mark
      className={cn('inline-flex items-center justify-center shrink-0', className)}
      {...props}
    >
      <svg viewBox="0 0 40 40" fill="none" className="h-full w-full" aria-hidden="true">
        <path d="M20 3 35 11.5v17L20 37 5 28.5v-17L20 3Z" stroke="currentColor" strokeWidth="2" />
        <path d="M11 20h4m-2-5v10m7-14v18m7-14v10m-2-5h4" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
        <circle cx="34" cy="9" r="4" fill="currentColor" />
      </svg>
    </span>
  );
}
