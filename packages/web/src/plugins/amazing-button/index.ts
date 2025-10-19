/**
 * Example Plugin: Amazing Button Plugin
 * This demonstrates how plugins work with the slot system
 */

import type { PluginManifest, ComponentBundle } from "@broccoli/sdk";
import { AmazingButton } from "./components/AmazingButton";

export const manifest: PluginManifest = {
    name: "amazing-button-plugin",
    version: "1.0.0",
    slots: [
        {
            name: "slots.header",
            position: "after",
            component: "components/AmazingButton",
        },
    ],
};

export const components: ComponentBundle = {
    "components/AmazingButton": AmazingButton,
};
