# @broccoli/sdk

Core SDK for the Broccoli plugin system with slot architecture.

## Installation

```bash
pnpm add @broccoli/sdk
```

## Usage

### Setting up the Provider

```tsx
import { PluginRegistryProvider } from "@broccoli/sdk/react";

function App() {
    return (
        <PluginRegistryProvider>
            <YourApp />
        </PluginRegistryProvider>
    );
}
```

### Creating Slots

```tsx
import { Slot } from "@broccoli/sdk/react";

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
import { usePluginRegistry } from "@broccoli/sdk/react";
import type { PluginManifest, ComponentBundle } from "@broccoli/sdk";

const manifest: PluginManifest = {
    name: "my-plugin",
    version: "1.0.0",
    slots: [
        {
            name: "slots.header",
            position: "after",
            component: "components/MyButton",
        },
    ],
};

const components: ComponentBundle = {
    "components/MyButton": MyButton,
};

function Setup() {
    const { registerPlugin } = usePluginRegistry();

    useEffect(() => {
        registerPlugin(manifest, components);
    }, [registerPlugin]);

    return null;
}
```

## API

### Types

-   `PluginManifest`: Plugin configuration
-   `SlotConfig`: Slot configuration
-   `ComponentBundle`: Component registry

### Components

-   `PluginRegistryProvider`: Context provider for plugin system
-   `Slot`: Slot component for plugin injection

### Hooks

-   `usePluginRegistry()`: Access plugin registry
-   `usePluginComponent(name)`: Get specific plugin component

## License

ISC
