interface E2EElement {
  waitForDisplayed(): Promise<boolean>;
  setValue(value: string): Promise<void>;
  getText(): Promise<string>;
}

interface E2EBrowser {
  readonly sessionId: string;
  execute<Result>(script: string): Promise<Result>;
  reloadSession(): Promise<string>;
  waitUntil(
    condition: () => Promise<boolean>,
    options: { readonly timeout: number; readonly timeoutMsg: string },
  ): Promise<boolean>;
}

declare const browser: E2EBrowser;
declare function $(selector: string): E2EElement;
