# host-assembly 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-004
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 代号由 CTR-PSR-001 声明）
> 上位依据：《代码契约规则》
> 适用范围：profile 与 bundle 的分层解析、条目合并与路径解析
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 读取一个 profile 目录下的 `profile.yml`。
- 按声明顺序叠加 bundle 补丁层，最后应用 profile 自身的补丁层。
- 按条目 id 合并各层，生成解析后的条目列表。
- 把条目中的相对路径解析为绝对路径，归一化 Client 面写法。

不负责：

- 实际装载插件。解析结果交给宿主生命周期能力消费。
- 校验模块路径是否存在、适配器是否已注册。
- 处理配置语义。`config` 原样透传。

## 对外操作

`loadProfile(dir: string) -> Promise<ResolvedEntry[]>`

- 输入条件：`dir` 下存在可解析的 `profile.yml`。
- 输出语义：返回按合并规则得到的条目数组，已过滤 `disabled` 条目；相对路径均基于各自声明文件所在目录解析。
- 操作级错误：见「错误」。

## 公共类型

`ProfileFile`（`profile.yml` 的结构）

- `bundles`：字符串数组，每项是相对 profile 目录的补丁文件路径。
- `plugins`：补丁条目数组，是 profile 自身的补丁层。

`PatchEntry`

- `id`：条目唯一标识，必填字符串。
- `module`：Host 面入口，相对声明它的文件，必填字符串。
- `client`：Client 面入口，可为字符串或 `{ entry, runtime }`。
- `runtime`：条目级执行环境适配器名。
- `config`：任意配置，原样透传。
- `disabled`：为 `true` 时该条目不出现在结果中。

`ResolvedEntry`

- 继承 `PatchEntry` 的其余字段。
- `modulePath`：`module` 解析后的绝对路径。
- `clientPath`：`client` 解析后的绝对路径，未声明时为 `undefined`。
- `clientRuntime`：归一化后的适配器名。

`ClientSource` 由宿主生命周期能力统一定义，本契约只规定归一化规则，不重复定义字段。

## 错误

当前实现不定义稳定错误标识，错误以 `Error` 的 message 表达：

- `invalid profile: <file>`：`profile.yml` 解析结果不是对象。
- `failed to read <file>: <原因>`：文件读取或 YAML 解析失败。
- `invalid patch entry in <file>`：补丁条目为空，或 `id` 不是字符串。
- `patch entry "<id>" is missing module in <file>`：补丁条目缺少字符串 `module`。
- `invalid client entry: expected string or { entry, runtime }`：`client` 形态非法。

## 整体保证

- 层序固定：`bundles` 按声明顺序先应用，profile 自身最后应用。
- 同 id 整条替换。后出现的层不与前层合并字段，而是整条覆盖。
- `disabled` 过滤发生在全部层合并之后。因此某层把条目标为 `disabled` 时，后续层若重新声明同一 id 且未标 `disabled`，该条目仍会出现在结果中。
- 相对路径相对声明文件解析。bundle 中的 `module` 与 `client` 相对该 bundle 文件，profile 自身的补丁相对 `profile.yml`。
- Client 面归一化顺序固定：对象形态的 `runtime` 优先于条目级 `runtime`，条目级优先于缺省值 `dom`。
- 未声明 `client` 的条目在结果中 `clientPath` 与 `clientRuntime` 都为 `undefined`。
- 输出不含重复 id。

## 按需扩展

校验时机：

- `module` 的必填校验在合并阶段执行，**早于 `disabled` 过滤**。因此一个 `disabled: true` 但缺少 `module` 的条目仍会导致整个解析失败。
- `client` 形态校验同样在合并阶段执行，与 `disabled` 无关。

未校验项：

- 解析结果不检查 `modulePath` 与 `clientPath` 指向的文件是否存在。
- 不检查 `clientRuntime` 是否为宿主已注册的适配器；未知适配器在装载阶段才失败。
- 不检查 `profile.yml` 中的 `bundles` 路径是否存在，缺失时以读取失败表达。

取证范围：

- `packages/pulsar-host/src/profile.ts`
- `packages/pulsar-host/src/types.ts`（`PatchEntry`、`ResolvedEntry`）

待决策项：

- `disabled` 条目是否应跳过字段校验。
- 是否需要在解析阶段校验模块文件存在性，把错误提前到装配阶段。
- 缺省适配器名 `dom` 是否应改为显式必填。
