// Test data shared by the list tests.

import type { Program } from "../ipc/bindings";

/** A bare application registered in HKLM 64-bit, with `extra` fields set. */
export function program(name: string, extra: Partial<Program> = {}): Program {
  return {
    id: `hklm64\\${name}`,
    name,
    publisher: null,
    version: null,
    installDate: null,
    estimatedSizeKib: null,
    installLocation: null,
    uninstallString: null,
    quietUninstallString: null,
    displayIcon: null,
    url: null,
    comments: null,
    msi: false,
    registryKey: `HKEY_LOCAL_MACHINE\\${name}`,
    scope: "machine64",
    profile: null,
    kind: "app",
    orphan: false,
    ...extra,
  };
}
