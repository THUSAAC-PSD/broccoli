import React, { type ReactNode } from 'react';

import { PluginErrorBoundary } from '@/plugin/PluginErrorBoundary';
import { usePluginRegistry } from '@/plugin/registry/use-plugin-registry';
import type { SlotConfig } from '@/plugin/types';
import { useSlotPermissions } from '@/slot/slot-permissions-context';

interface SlotProps {
  name: string;
  as?: React.ElementType;
  className?: string;
  children?: ReactNode;
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
  slotProps = {},
}: SlotProps) {
  const { getSlots, components } = usePluginRegistry();
  const slotPermissions = useSlotPermissions();
  const userPermissions = slotPermissions?.permissions ?? [];

  const allSlots = getSlots(name);

  // Filter out slots that require a permission the user doesn't have.
  // Slots without a `permission` field are visible to everyone.
  const slots = allSlots.filter((slot) => {
    if (!slot.permission) return true;
    return userPermissions.includes(slot.permission);
  });

  const Container = as;

  // Render slots based on their position
  const renderSlot = (slot: SlotConfig, index: number) => {
    const SlotComponent = components[slot.component];
    if (!SlotComponent) {
      console.warn(`Component ${slot.component} not found for slot ${name}`);
      return null;
    }

    return (
      <PluginErrorBoundary
        key={`${slot.name}-${slot.component}-${index}`}
        pluginName={slot._pluginName ?? 'unknown'}
        componentName={slot.component}
      >
        <SlotComponent {...slotProps} />
      </PluginErrorBoundary>
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
        children: content,
      };
      content = (
        <PluginErrorBoundary
          pluginName={slot._pluginName ?? 'unknown'}
          componentName={slot.component}
        >
          <WrapperComponent {...wrapperProps} />
        </PluginErrorBoundary>
      );
    }
  });

  return (
    <>
      {prependSlots.map(renderSlot)}
      <Container className={className}>{content}</Container>
      {appendSlots.map(renderSlot)}
    </>
  );
}
