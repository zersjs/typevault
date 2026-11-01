import * as SelectPrimitive from "@radix-ui/react-select";
import * as SliderPrimitive from "@radix-ui/react-slider";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as CheckboxPrimitive from "@radix-ui/react-checkbox";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Check, ChevronDown } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

const NONE = "__none";

export function Select({ value, onChange, options, className, placeholder }: {
  value: string | null | undefined;
  onChange: (value: string | null) => void;
  options: { value: string | null; label: string }[];
  className?: string;
  placeholder?: string;
}) {
  return (
    <SelectPrimitive.Root value={value ?? NONE} onValueChange={(v) => onChange(v === NONE ? null : v)}>
      <SelectPrimitive.Trigger
        className={cn("inline-flex h-7 items-center gap-1.5 rounded-md border border-line bg-raised px-2.5 text-xs text-ink hover:bg-hover", className)}
      >
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon className="ml-auto"><ChevronDown className="size-3 text-faint" /></SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content position="popper" sideOffset={4} className="anim-fade z-50 max-h-80 min-w-36 overflow-hidden rounded-lg border border-line-strong bg-raised shadow-[var(--shadow)]">
          <SelectPrimitive.Viewport className="p-1">
            {options.map((o) => (
              <SelectPrimitive.Item key={o.value ?? NONE} value={o.value ?? NONE} className="flex h-7 items-center justify-between gap-4 rounded-[5px] px-2 text-xs text-ink data-[highlighted]:bg-hover">
                <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                <SelectPrimitive.ItemIndicator><Check className="size-3" /></SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

export function Slider({ value, onChange, min, max, step = 1, className, label }: {
  value: number; onChange: (v: number) => void; min: number; max: number; step?: number; className?: string; label: string;
}) {
  return (
    <SliderPrimitive.Root
      className={cn("relative flex h-4 w-full touch-none items-center", className)}
      value={[value]} min={min} max={max} step={step} onValueChange={([v]) => onChange(v)}
    >
      <SliderPrimitive.Track className="relative h-[3px] grow overflow-hidden rounded-full bg-line-strong">
        <SliderPrimitive.Range className="absolute h-full bg-soft" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb aria-label={label} className="block size-3 rounded-full bg-accent shadow" />
    </SliderPrimitive.Root>
  );
}

export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <SwitchPrimitive.Root
      aria-label={label} checked={checked} onCheckedChange={onChange}
      className="relative h-[18px] w-[30px] rounded-full bg-line-strong transition-colors data-[state=checked]:bg-accent"
    >
      <SwitchPrimitive.Thumb className="block size-3.5 translate-x-0.5 rounded-full bg-white shadow transition-transform data-[state=checked]:translate-x-[14px] data-[state=checked]:bg-[var(--on-accent)]" />
    </SwitchPrimitive.Root>
  );
}

export function Checkbox({ checked, onChange, label, className }: { checked: boolean; onChange: () => void; label: string; className?: string }) {
  return (
    <CheckboxPrimitive.Root
      aria-label={label} checked={checked} onCheckedChange={onChange} onClick={(e) => e.stopPropagation()}
      className={cn("flex size-4 items-center justify-center rounded-[4px] border border-line-strong bg-panel data-[state=checked]:border-accent data-[state=checked]:bg-accent", className)}
    >
      <CheckboxPrimitive.Indicator><Check className="size-3 text-on-accent" strokeWidth={3} /></CheckboxPrimitive.Indicator>
    </CheckboxPrimitive.Root>
  );
}

export function Tooltip({ text, children }: { text: string; children: ReactNode }) {
  return (
    <TooltipPrimitive.Root delayDuration={350}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content sideOffset={5} className="anim-fade z-50 max-w-64 rounded-md border border-line-strong bg-raised px-2 py-1 text-[11px] text-ink shadow-[var(--shadow)]">
          {text}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

export const Menu = DropdownPrimitive.Root;
export const MenuTrigger = DropdownPrimitive.Trigger;

export function MenuContent({ children, align = "end" }: { children: ReactNode; align?: "start" | "end" }) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content align={align} sideOffset={4} className="anim-fade z-50 max-h-80 min-w-44 overflow-auto rounded-lg border border-line-strong bg-raised p-1 shadow-[var(--shadow)]">
        {children}
      </DropdownPrimitive.Content>
    </DropdownPrimitive.Portal>
  );
}

export function MenuItem({ children, onSelect, danger, disabled }: { children: ReactNode; onSelect: () => void; danger?: boolean; disabled?: boolean }) {
  return (
    <DropdownPrimitive.Item
      disabled={disabled} onSelect={onSelect}
      className={cn("flex h-7 items-center gap-2 rounded-[5px] px-2 text-xs data-[highlighted]:bg-hover data-[disabled]:opacity-40 [&_svg]:size-3.5", danger ? "text-bad" : "text-ink")}
    >
      {children}
    </DropdownPrimitive.Item>
  );
}

export const MenuSeparator = () => <DropdownPrimitive.Separator className="my-1 h-px bg-line" />;

export function Modal({ open, onClose, title, description, children }: {
  open: boolean; onClose: () => void; title: string; description?: string; children: ReactNode;
}) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="anim-fade fixed inset-0 z-40 bg-black/45" />
        <DialogPrimitive.Content
          aria-describedby={undefined}
          onFocusOutside={(e) => e.preventDefault()}
          className="anim-rise fixed left-1/2 top-[28%] z-50 w-[400px] -translate-x-1/2 rounded-xl border border-line-strong bg-raised p-5 shadow-[var(--shadow)]">
          <DialogPrimitive.Title className="text-[15px] font-semibold">{title}</DialogPrimitive.Title>
          {description && <p className="mt-1.5 text-[13px] leading-relaxed text-soft">{description}</p>}
          <div className="mt-4">{children}</div>
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
