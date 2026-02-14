/**
 * @broccoli/sdk/react
 * React-specific exports and hooks
 */

import React, { type ReactNode } from 'react';

import { usePluginRegistry } from '@/plugin/use-plugin-registry';
import type { SlotConfig } from '@/types';

// Slot Component
interface SlotProps<TContext = unknown> {
  name: string;
  as?: React.ElementType;
  className?: string;
  children?: ReactNode;
  /**
   * Context object passed to slot condition functions
   */
  context?: TContext;
  /**
   * Additional props to pass to all slot components
   */
  slotProps?: Record<string, unknown>;
}

export function Slot({
  name,
  as = 'div',
  className,
  children,
  context,
  slotProps = {},
}: SlotProps) {
  const { getSlots, components } = usePluginRegistry();
  const slots = getSlots(name, context);
  const Component = as;

  // Render slots based on their position
  const renderSlot = (slot: SlotConfig, index: number) => {
    const SlotComponent = components[slot.component];
    if (!SlotComponent) {
      console.warn(`Component ${slot.component} not found for slot ${name}`);
      return null;
    }

    // Merge slot props with component props
    const componentProps = {
      ...slotProps,
      ...slot.props,
    };

    return (
      <SlotComponent
        key={`${slot.name}-${slot.component}-${index}`}
        {...componentProps}
      />
    );
  };

  // Group slots by position
  const replaceSlots = slots.filter((s) => s.position === 'replace');
  const wrapSlots = slots.filter((s) => s.position === 'wrap');
  const prependSlots = slots.filter((s) => s.position === 'prepend');
  const beforeSlots = slots.filter((s) => s.position === 'before');
  const afterSlots = slots.filter((s) => s.position === 'after');
  const appendSlots = slots.filter((s) => s.position === 'append');

  // Build content based on position types
  let content: ReactNode;

  // If there are replace slots, use them instead of children
  if (replaceSlots.length > 0) {
    content = replaceSlots.map(renderSlot);
  } else {
    // Normal flow: prepend, before, children, after, append
    content = (
      <>
        {beforeSlots.map(renderSlot)}
        {children}
        {afterSlots.map(renderSlot)}
      </>
    );
  }

  // Apply wrap slots (from outermost to innermost)
  wrapSlots.reverse().forEach((slot) => {
    const WrapperComponent = components[slot.component];
    if (WrapperComponent) {
      const wrapperProps = {
        ...slotProps,
        ...slot.props,
        children: content,
      };
      content = <WrapperComponent {...wrapperProps} />;
    }
  });

  return (
    <>
      {prependSlots.map(renderSlot)}
      <Component className={className}>{content}</Component>
      {appendSlots.map(renderSlot)}
    </>
  );
}
