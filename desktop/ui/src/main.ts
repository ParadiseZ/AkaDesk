import { appShellName } from "./app/AppShell";

export function bootstrap(): string {
  return `${appShellName} ready for M2`;
}

console.log(bootstrap());
