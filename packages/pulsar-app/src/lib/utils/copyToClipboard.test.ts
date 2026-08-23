/**
 * CopyToClipboard 单元测试。
 *
 * 运行方式（Node 24，type stripping 默认开启；在 projects/pulsar-app 下执行）：
 *   node --test src/lib/utils/copyToClipboard.test.ts
 *
 * 覆盖三条分支：
 *  1. Clipboard API 可用 → 走 writeText，返回 true
 *  2. Clipboard API 失败 → 回退 execCommand，返回 true
 *  3. 空文本 / 两种方案均失败 → 返回 false
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { CopyToClipboard } from "./copyToClipboard.impl.ts";

/** 在 Node 的 `globalThis` 上安全地定义可写属性（navigator/document 原是 only-getter）。 */
function defineGlobal(name: string, value: unknown): void {
  Object.defineProperty(globalThis, name, {
    value,
    writable: true,
    configurable: true,
    enumerable: true,
  });
}

/** 复制路径用到的 document 替身的最小接口。 */
interface FakeDocument {
  createElement(): {
    value: string;
    setAttribute(): void;
    style: Record<string, string>;
    focus(): void;
    select(): void;
    remove(): void;
  };
  body: { appendChild(): void };
  execCommand(): boolean;
  __getRemoved(): boolean;
}

/** 构造一个可注入行为的 document 替身（仅暴露复制路径用到的方法/字段）。 */
function makeDocument(execReturns: boolean): FakeDocument {
  let removed = false;
  return {
    createElement() {
      return {
        value: "",
        setAttribute() {},
        style: {},
        focus() {},
        select() {},
        remove() {
          removed = true;
        },
      };
    },
    body: { appendChild() {} },
    execCommand() {
      return execReturns;
    },
    __getRemoved() {
      return removed;
    },
  } as unknown as FakeDocument;
}

test("Clipboard API 可用时使用 writeText 并返回 true", async () => {
  const written: string[] = [];
  defineGlobal("navigator", {
    clipboard: {
      writeText: async (t: string) => {
        written.push(t);
      },
    },
  });
  defineGlobal("document", makeDocument(false));

  const ok = await CopyToClipboard.copyText("hello");
  assert.equal(ok, true);
  assert.deepEqual(written, ["hello"]);
});

test("Clipboard API 失败时回退 execCommand 并返回 true", async () => {
  const doc = makeDocument(true);
  defineGlobal("navigator", {
    clipboard: {
      writeText: async () => {
        throw new Error("denied");
      },
    },
  });
  defineGlobal("document", doc);

  const ok = await CopyToClipboard.copyText("fallback");
  assert.equal(ok, true);
  assert.equal(doc.__getRemoved(), true, "textarea 应被移除");
});

test("空文本返回 false，不触发任何写剪贴板", async () => {
  let writeCalled = false;
  defineGlobal("navigator", {
    clipboard: {
      writeText: async () => {
        writeCalled = true;
      },
    },
  });
  defineGlobal("document", makeDocument(true));

  const ok = await CopyToClipboard.copyText("");
  assert.equal(ok, false);
  assert.equal(writeCalled, false);
});

test("两种方案均失败（execCommand 返回 false）时返回 false", async () => {
  defineGlobal("navigator", {
    clipboard: {
      writeText: async () => {
        throw new Error("denied");
      },
    },
  });
  defineGlobal("document", makeDocument(false));

  const ok = await CopyToClipboard.copyText("x");
  assert.equal(ok, false);
});
