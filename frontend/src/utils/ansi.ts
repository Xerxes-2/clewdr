// Utility to strip ANSI escape codes from strings
// Matches sequences like \u001b[32m ... \u001b[0m
const ansiRegex = /\u001B\[[0-?]*[ -\/]*[@-~]/g;

export function stripAnsi(input: string): string {
  if (!input) return "";
  try {
    return input.replace(ansiRegex, "");
  } catch {
    return input;
  }
}

