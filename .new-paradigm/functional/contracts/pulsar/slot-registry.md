# slot-registry 代码契约

> 文档状态：待确认
> 契约标识：CTR-PSR-001
> 所属领域：pulsar（Pulsar 插件宿主与前端插件体系；`PSR` 为本领域契约代号，本文件为该领域首份契约，据此声明）
> 上位依据：《代码契约规则》
> 适用范围：浏览器侧插槽注册表 `SlotRegistry` 的登记、渲染、校验与销毁
> 下位产物：实现代码、契约一致性检查结果

## 职责边界

负责：

- 维护一组插槽注册项，每项由槽名、唯一 id、可选作用域、排序值和子槽声明构成。
- 在调用方指定的 DOM 容器上，把槽名匹配且作用域满足的注册项实例化为组件。
- 记录每次实例化产生的节点与清理回调，并在卸载时按逆序执行清理、移除节点。
- 报告注册到了无人声明槽位的注册项。

不负责：

- 决定渲染时机。没有任何自动触发：不监听注册变更，不监听容器变化，只在被调用时执行。
- 校验注册时槽是否已被声明。
- 声明或管理子槽的语义（子槽由父注册项的 `children` 与父组件调用 `renderSlot` 共同确定）。
- 页面级依赖声明、插件启停与作用域变更通知。
- 服务端侧的面板贡献，该职责属于宿主侧 `PanelService`。

## 对外操作

`register(spec: SlotSpec, component: SlotComponent) -> dispose: () => void`

- 输入条件：`spec.name`、`spec.id` 均为字符串；`component` 为函数；`spec.id` 在当前注册表中未使用。
- 输出语义：`dispose` 幂等，注销该注册项并卸载它在所有容器上的实例。
- 操作级错误：见「错误」。

`render(name: string, container: Element, options?: { scopeId?: string }) -> void`

- 输入条件：`name` 必须是根级固定声明 `root`，或父注册项 `children` 中的一项；`container` 为可写的 `Element`。
- 输出语义：调用返回时，`container` 内属于 `name` 的上一次实例已被卸载，新实例已按排序全部挂载。
- 操作级错误：父级未声明该槽名时抛错；作用域值非法时抛错。

`validate() -> string[]`

- 无输入条件。
- 输出语义：返回槽名不被任何注册项的 `children` 覆盖、且不等于 `root` 的注册项 id；每次调用重新计算，不缓存。
- 操作级错误：无。

`dispose() -> void`

- 无输入条件。
- 输出语义：卸载全部已渲染实例、清空注册表；调用后注册表为空，`validate()` 返回空数组。
- 操作级错误：无。

`view.renderSlot(name: string, container: Element, options?: { scopeId?: string }) -> void`

- 输入条件：调用方必须是某个注册项的组件函数，即持有该次实例的 `SlotView`。
- 输出语义：等价于以当前注册项为父级调用 `render`，声明集合取该注册项的 `children`。
- 操作级错误：父级未声明该槽名时抛错。

`view.onCleanup(fn: () => void) -> void`

- 输入条件：`fn` 为函数。
- 输出语义：登记到当前实例记录，在该实例卸载时按后进先出执行一次。
- 操作级错误：无。

## 公共类型

`SlotSpec`

- `name`：槽名。父级通过 `children` 声明后才允许挂载；`root` 恒可用。
- `id`：注册项唯一标识，同一注册表内重复即失败。
- `scope`：`root` | `scoped` | `scoped-maybe`，缺省视为不限制。
- `order`：同级渲染顺序，数值小的先渲染，缺省按 0 参与排序。
- `children`：本注册项允许承载的子槽名集合，缺省为空。

`SlotView`

- `scopeId`：本次渲染的作用域标识，根作用域为 `undefined`。
- `renderSlot`：向当前实例容器继续渲染子槽。
- `onCleanup`：登记实例级清理回调。

`SlotComponent`

- 签名 `(container: Element, view: SlotView) -> Element | void | () => void`。
- 返回元素时视为实例产出的根节点；返回函数时视为实例级卸载函数；返回空表示无额外产出。

`LegacySlotComponent`

- 签名 `(container: Element, view: SlotView) -> void | () => void`。
- 与 `SlotComponent` 的差别仅在约定：旧组件自行向容器追加节点，不返回根节点，因此需经包装后使用。

`SlotRegistryApi`

- `register`、`render`、`validate`、`dispose` 四项，语义同上。

## 错误

当前实现只以 `Error` 的 message 表达失败，**不提供稳定错误标识**。触发条件与现有 message 如下：

- `invalid slot registration`：`register` 收到的 `spec` 缺少字符串 `name` 或 `id`。
- `slot <id> has no component`：`register` 收到的 `component` 不是函数。
- `duplicate slot id: <id>`：`register` 的 `id` 与已登记项重复。
- `slot not declared by parent: <name>`：`render` 或 `renderSlot` 的目标槽名不在父级声明集合内；根级声明集合固定为 `["root"]`。
- `unknown slot scope: <scope>`：注册项的 `scope` 不在三个合法值内，在渲染时触发。

## 整体保证

- 注册与渲染相互独立。`render` 只在其被调用的时刻执行；注册、注销或已渲染内容的变化都不会自行触发新的渲染。
- `render` 对被渲染内容幂等。同一容器同一槽名再次调用时，先卸载该容器该槽名下的全部记录，再按当次注册表快照全量挂载。
- `render` 使用调用时刻的注册表快照。即使挂载过程中有新的 `register` 发生，该注册项也不出现在本次 `render` 的结果中。
- 已完成的渲染不会因后续 `register` 而更新。新注册项要进入某个容器，必须由该槽的下一次 `render` 或 `renderSlot` 调用带入。
- 渲染顺序只由 `order` 决定，`order` 相同者保持注册表内的登记顺序。
- 每个实例记录独立卸载。卸载时依次执行组件返回的卸载函数、逆序执行 `onCleanup` 回调、移除该实例新增的节点；重复卸载不产生额外副作用。
- 注销一个注册项会卸载它在所有容器上的全部实例，但不会因此触发其他任何渲染。
- 注册不校验槽是否存在。槽名无人声明时注册仍然成功，只在显式调用 `validate()` 时才被报告。
- `validate()` 只依据槽名声明关系判断，不依据是否已渲染；被声明但从未渲染的槽不产生报告。

## 按需扩展

作用域匹配：

- `root`：始终匹配。
- `scoped-maybe`：始终匹配。
- `scoped`：仅当本次渲染传入非 `undefined` 的 `scopeId` 时匹配。
- 缺省：视为不限制，始终匹配。

渲染记录的归属：

- 记录以容器元素为键，以槽名为二级键保存。同一注册项被渲染到多个容器时，会存在多份独立记录，每次都会执行一次组件函数。
- 父实例卸载时只移除父实例自身新增的节点，不递归卸载子槽记录。子槽记录继续留在表中，直到其注册项被注销、所在容器被再次渲染同一槽名、或调用 `dispose()`。
- 因此，父级容器被移除后，其子槽记录不会自动清理；这是当前可观察行为。

取证范围：

- `packages/pulsar-plugin-client/src/slots.ts`（行为主体）
- `packages/pulsar-plugin-client/src/types.ts`（公共类型）
- `packages/pulsar-host/client/runtime.js`（宿主侧唯一渲染调用点与启动后孤儿报告）

待决策项：

- 是否引入稳定错误标识。现状只有 message，调用方无法按标识分支处理。
- 是否提供注册变更通知。现状没有任何通知机制，运行期新增注册项的可见性完全依赖调用方再次渲染。
- 父实例卸载是否应递归卸载子槽记录。现状为否，会导致容器移除后的记录滞留。
