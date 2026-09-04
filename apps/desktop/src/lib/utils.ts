import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export type WithoutUndefined<T extends object> = {
  [K in keyof T as undefined extends T[K] ? K : never]?: Exclude<T[K], undefined>;
} & {
  [K in keyof T as undefined extends T[K] ? never : K]: T[K];
};

export function withoutUndefined<T extends object>(value: T): WithoutUndefined<T> {
  return Object.fromEntries(
    Object.entries(value).filter((entry) => entry[1] !== undefined),
  ) as WithoutUndefined<T>;
}
export function withoutClass<T extends { class?: unknown }>(value: T): Omit<T, "class"> {
  return Object.fromEntries(Object.entries(value).filter((entry) => entry[0] !== "class")) as Omit<
    T,
    "class"
  >;
}

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
