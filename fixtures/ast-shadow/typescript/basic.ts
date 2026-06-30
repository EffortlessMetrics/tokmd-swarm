import { readFile } from "node:fs/promises";

export function compute(value: number): number {
  if (value === 0) {
    return 0;
  }

  for (let item = 0; item < value; item += 1) {
    while (item > 1) {
      break;
    }
  }

  switch (value) {
    case 1:
      return 1;
    default:
      return value;
  }
}
