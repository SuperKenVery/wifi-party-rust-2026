I will implement the architecture by making `PushNode` and `PullNode` traits chainable, allowing `AudioPipeline` to coordinate the flow between `node` (logic) and `inner` (next stage).

1. **Modify Traits in** **`src/pipeline/node/mod.rs`**

   * Update `PushNode` to `trait PushNode<Next> { fn push(&mut self, frame, inner: &mut Next) where Next: PushNode; }`.

   * Update `PullNode` to `trait PullNode<Next> { fn pull(&mut self, inner: &mut Next) -> Option<Frame> where Next: PullNode; }`.

2. **Refactor** **`src/pipeline/layer.rs`**

   * Rename `AudioLayer` to `AudioPipeline`.

   * Structure: `pub struct AudioPipeline<Inner, Node> { pub next: Next, pub node: Node }`.

   * Implement `fn push` for `AudioPipeline` when self.node is PushNode by delegating to `self.node.push(frame, &mut self.next)`.

   * Implement `fn pull` for `AudioPipeline` when self.node is PullNode by delegating to `self.node.pull(&mut self.next)`.

   * Add blanket implementations for `AudioEffect`

     * `PushNode<I>`: Process frame, then `inner.push(frame, &mut ())`.

     * `PullNode<I>`: `inner.pull(&mut ())`, then process frame.

     * `put trait AudioEffect<T, SampleRate, Channels> { fn process(data: AudioBuffer<...>); }`

3. **Update Existing Nodes**

   * Update `Mixer`, `NetworkPush`, etc., to implement `PushNode<()>` or `PullNode<()>` (ignoring the inner argument as they are terminals).

4. **Update** **`src/pipeline/pipeline.rs`**

   * Switch to using `AudioPipeline`.

This architecture satisfies your requirement that `AudioPipeline` wraps an inner pipeline, and push/pull logic is determined by the `node` (which can be an `AudioEffect` or other custom implementation).
