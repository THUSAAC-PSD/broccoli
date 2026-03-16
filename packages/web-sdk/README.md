# @broccoli/web-sdk

Core SDK for the Broccoli plugin system with slot architecture.

## Installation

```bash
pnpm add @broccoli/web-sdk
```

## Usage

### Setting up the Provider

```tsx
import { PluginRegistryProvider } from '@broccoli/web-sdk/plugin';

function App() {
  return (
    <PluginRegistryProvider backendUrl="http://127.0.0.1:3000">
      <YourApp />
    </PluginRegistryProvider>
  );
}
```

### Creating Slots

```tsx
import { Slot } from '@broccoli/web-sdk/slot';

function Header() {
  return (
    <Slot name="slots.header" className="flex gap-4">
      <button>Default Button</button>
    </Slot>
  );
}
```

### Registering Plugins

```tsx
import {
  PluginRegistryProvider,
  type ComponentBundle,
  type PluginModule,
} from '@broccoli/web-sdk/plugin';

function MyButton() {
  return null;
}

const components: ComponentBundle = {
  'components/MyButton': MyButton,
};

const plugin: PluginModule = {
  manifest: {
    id: 'my-plugin',
    name: 'my-plugin',
    version: '1.0.0',
    components: {
      'components/MyButton': 'MyButton',
    },
    slots: [
      {
        name: 'slots.header',
        position: 'after',
        component: 'components/MyButton',
      },
    ],
  },
  MyButton,
};

function App() {
  return (
    <PluginRegistryProvider
      backendUrl="http://127.0.0.1:3000"
      pluginModules={[plugin]}
    >
      <YourApp />
    </PluginRegistryProvider>
  );
}
```

## API

### Types

- `ActivePluginManifest`: Plugin manifest shape
- `SlotConfig`: Slot configuration
- `ComponentBundle`: Component registry
- `PluginModule`: Runtime plugin module

### Components

- `PluginRegistryProvider`: Context provider for plugin system
- `Slot`: Slot component for plugin injection

### Hooks

- `usePluginRegistry()`: Access plugin registry
- `usePluginComponent(name)`: Get specific plugin component

## License

MIT
