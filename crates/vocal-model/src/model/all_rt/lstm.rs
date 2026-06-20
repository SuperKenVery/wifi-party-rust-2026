use super::*;

#[cubecl::cube(launch_unchecked, address_type = "dynamic")]
fn lstm_step_kernel<F: Float>(
    x_proj: &LinearView<F>,
    w_h: &LinearView<F>,
    b_h: &LinearView<F>,
    h_prev: &LinearView<F>,
    c_prev: &LinearView<F>,
    h_next: &mut LinearView<F, ReadWrite>,
    c_next: &mut LinearView<F, ReadWrite>,
    output: &mut LinearView<F, ReadWrite>,
    t: usize,
    batch: usize,
    hidden: usize,
    #[define(F)] _dtype: StorageType,
) {
    let pos = ABSOLUTE_POS;
    if pos >= batch * hidden {
        terminate!();
    }

    let b = pos / hidden;
    let h = pos - b * hidden;
    let gates = hidden * 4;
    let x_base = (t * batch + b) * gates + h;

    let mut input_gate = x_proj[x_base] + b_h[h];
    let mut forget_gate = x_proj[x_base + hidden] + b_h[hidden + h];
    let mut cell_gate = x_proj[x_base + hidden * 2] + b_h[hidden * 2 + h];
    let mut output_gate = x_proj[x_base + hidden * 3] + b_h[hidden * 3 + h];

    for k in 0..hidden {
        let h_value = h_prev[b * hidden + k];
        let w_base = k * gates + h;
        input_gate += h_value * w_h[w_base];
        forget_gate += h_value * w_h[w_base + hidden];
        cell_gate += h_value * w_h[w_base + hidden * 2];
        output_gate += h_value * w_h[w_base + hidden * 3];
    }

    let one = F::new(1.0);
    let input_gate = one / (one + (-input_gate).exp());
    let forget_gate = one / (one + (-forget_gate).exp());
    let cell_gate = cell_gate.tanh();
    let output_gate = one / (one + (-output_gate).exp());

    let state_pos = b * hidden + h;
    let cell = forget_gate * c_prev[state_pos] + input_gate * cell_gate;
    let hidden_value = output_gate * cell.tanh();

    c_next[state_pos] = cell;
    h_next[state_pos] = hidden_value;
    output[(t * batch + b) * hidden + h] = hidden_value;
}

fn cube_tensor<B: CustomLstmBackend, const D: usize>(
    tensor: Tensor<B, D>,
) -> CubeTensor<WgpuRuntime> {
    tensor.into_primitive().tensor()
}

fn burn_tensor<B: CustomLstmBackend, const D: usize>(
    tensor: CubeTensor<WgpuRuntime>,
) -> Tensor<B, D> {
    Tensor::from_primitive(TensorPrimitive::Float(tensor))
}

fn lstm_step_address_type(tensors: &[&CubeTensor<WgpuRuntime>]) -> AddressType {
    tensors
        .iter()
        .map(|tensor| tensor.required_address_type())
        .max()
        .unwrap_or(AddressType::U32)
}

fn launch_lstm_step_f32(
    x_proj: &CubeTensor<WgpuRuntime>,
    w_h: &CubeTensor<WgpuRuntime>,
    b_h: &CubeTensor<WgpuRuntime>,
    h_prev: &CubeTensor<WgpuRuntime>,
    c_prev: &CubeTensor<WgpuRuntime>,
    h_next: &CubeTensor<WgpuRuntime>,
    c_next: &CubeTensor<WgpuRuntime>,
    output: &CubeTensor<WgpuRuntime>,
    t: usize,
    batch: usize,
    hidden: usize,
) {
    let working_units = batch * hidden;
    let cube_dim = CubeDim::new(&x_proj.client, working_units);
    let cube_count = calculate_cube_count_elemwise(&x_proj.client, working_units, cube_dim);
    let address_type =
        lstm_step_address_type(&[x_proj, w_h, b_h, h_prev, c_prev, h_next, c_next, output]);

    unsafe {
        lstm_step_kernel::launch_unchecked::<WgpuRuntime>(
            &x_proj.client,
            cube_count,
            cube_dim,
            address_type,
            x_proj.clone().into_linear_view(),
            w_h.clone().into_linear_view(),
            b_h.clone().into_linear_view(),
            h_prev.clone().into_linear_view(),
            c_prev.clone().into_linear_view(),
            h_next.clone().into_linear_view(),
            c_next.clone().into_linear_view(),
            output.clone().into_linear_view(),
            t,
            batch,
            hidden,
            x_proj.dtype.into(),
        );
    }
}

/// Optimized LSTM forward pass.
///
/// Reduces GPU dispatches from ~11/step to ~3/step by:
/// 1. Pre-projecting all input steps in ONE batched matmul (amortised over seq_len)
/// 2. Concatenating all 4 gate hidden weights so hidden projection is ONE matmul/step
///
/// Input/output convention: `batch_first = false`, i.e. `[seq, batch, features]`.
fn lstm_preproj_burn<B: Backend>(
    lstm: &burn::nn::Lstm<B>,
    input: Tensor<B, 3>,            // [seq, batch, input_size]
    state: Option<LstmState<B, 2>>, // h/c each [batch, hidden]
) -> (Tensor<B, 3>, LstmState<B, 2>) {
    let [seq, batch, input_size] = input.dims();
    let hidden = lstm.d_hidden;
    let device = input.device();

    // ── 1. Pre-project all seq steps in one GEMM ──────────────────────────────
    // LinearLayout::Row stores weight as [d_input, d_output].
    // Concat along dim=1: [d_input, 4*d_output] = [input_size, 4*hidden]
    let w_x = Tensor::cat(
        vec![
            lstm.input_gate.input_transform.weight.val(),
            lstm.forget_gate.input_transform.weight.val(),
            lstm.cell_gate.input_transform.weight.val(),
            lstm.output_gate.input_transform.weight.val(),
        ],
        1,
    ); // [input_size, 4*hidden]
       // Flatten seq*batch; reshape handles non-contiguous inputs (implicit contiguous copy)
    let input_flat = input.reshape([seq * batch, input_size]); // [seq*batch, input_size]
                                                               // [seq*batch, input_size] @ [input_size, 4*hidden]  →  [seq*batch, 4*hidden]
    let mut x_proj_flat = input_flat.matmul(w_x);
    // Add input biases (all 4 gates combined)
    if let Some(b) = lstm.input_gate.input_transform.bias.as_ref() {
        let b_x = Tensor::cat(
            vec![
                b.val(),
                lstm.forget_gate
                    .input_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
                lstm.cell_gate.input_transform.bias.as_ref().unwrap().val(),
                lstm.output_gate
                    .input_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
            ],
            0,
        ); // [4*hidden]
        x_proj_flat = x_proj_flat + b_x.unsqueeze_dims::<2>(&[0]); // broadcast over batch
    }
    let x_proj = x_proj_flat.reshape([seq, batch, 4 * hidden]); // [seq, batch, 4*hidden]

    // ── 2. Concat hidden weights (computed once, shared across all steps) ──────
    // [d_input=hidden, 4*d_output=4*hidden] after dim=1 concat
    let w_h = Tensor::cat(
        vec![
            lstm.input_gate.hidden_transform.weight.val(),
            lstm.forget_gate.hidden_transform.weight.val(),
            lstm.cell_gate.hidden_transform.weight.val(),
            lstm.output_gate.hidden_transform.weight.val(),
        ],
        1,
    ); // [hidden, 4*hidden]

    let b_h = lstm.input_gate.hidden_transform.bias.as_ref().map(|_| {
        Tensor::cat(
            vec![
                lstm.input_gate
                    .hidden_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
                lstm.forget_gate
                    .hidden_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
                lstm.cell_gate.hidden_transform.bias.as_ref().unwrap().val(),
                lstm.output_gate
                    .hidden_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
            ],
            0,
        ) // [4*hidden]
    });

    // ── 3. Initialise h, c ────────────────────────────────────────────────────
    let (mut c, mut h) = match state {
        Some(s) => (s.cell, s.hidden),
        None => (
            Tensor::zeros([batch, hidden], &device),
            Tensor::zeros([batch, hidden], &device),
        ),
    };

    // ── 4. Sequential recurrence over time steps ──────────────────────────────
    let mut output_steps = Vec::with_capacity(seq);

    for t in 0..seq {
        // Pre-projected input for step t: [batch, 4*hidden]
        let x_t = x_proj.clone().narrow(0, t, 1).squeeze_dims::<2>(&[0]);

        // Hidden projection: ONE matmul → [batch, 4*hidden]
        let mut gates = x_t + h.clone().matmul(w_h.clone());
        if let Some(ref b) = b_h {
            gates = gates + b.clone().unsqueeze_dims::<2>(&[0]);
        }

        // The generated model uses the default LSTM activations: sigmoid for
        // input/forget/output gates and tanh for cell/hidden state. Applying
        // each activation to the packed gate tensor trades a little extra math
        // for fewer tiny GPU dispatches in the recurrent loop.
        let gates_sigmoid = burn::tensor::activation::sigmoid(gates.clone());
        let gates_tanh = gates.tanh();
        let i = gates_sigmoid.clone().narrow(1, 0, hidden);
        let f = gates_sigmoid.clone().narrow(1, hidden, hidden);
        let g = gates_tanh.narrow(1, 2 * hidden, hidden);
        let o = gates_sigmoid.narrow(1, 3 * hidden, hidden);

        c = f * c + i * g;
        if let Some(clip) = lstm.clip {
            c = c.clamp(-clip as f32, clip as f32);
        }
        h = o * c.clone().tanh();

        output_steps.push(h.clone().unsqueeze_dims::<3>(&[0]));
    }

    let output = Tensor::cat(output_steps, 0);

    (output, LstmState::new(c, h))
}

pub(super) fn lstm_preproj<B: CustomLstmBackend>(
    lstm: &burn::nn::Lstm<B>,
    input: Tensor<B, 3>,            // [seq, batch, input_size]
    state: Option<LstmState<B, 2>>, // h/c each [batch, hidden]
) -> (Tensor<B, 3>, LstmState<B, 2>) {
    if lstm.batch_first
        || lstm.reverse
        || lstm.input_forget
        || lstm.clip.is_some()
        || lstm.input_gate.hidden_transform.bias.is_none()
    {
        return lstm_preproj_burn(lstm, input, state);
    }

    let [seq, batch, input_size] = input.dims();
    let hidden = lstm.d_hidden;
    let device = input.device();
    let dtype = input.dtype();

    let w_x = Tensor::cat(
        vec![
            lstm.input_gate.input_transform.weight.val(),
            lstm.forget_gate.input_transform.weight.val(),
            lstm.cell_gate.input_transform.weight.val(),
            lstm.output_gate.input_transform.weight.val(),
        ],
        1,
    );
    let input_flat = input.reshape([seq * batch, input_size]);
    let mut x_proj_flat = input_flat.matmul(w_x);
    if let Some(b) = lstm.input_gate.input_transform.bias.as_ref() {
        let b_x = Tensor::cat(
            vec![
                b.val(),
                lstm.forget_gate
                    .input_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
                lstm.cell_gate.input_transform.bias.as_ref().unwrap().val(),
                lstm.output_gate
                    .input_transform
                    .bias
                    .as_ref()
                    .unwrap()
                    .val(),
            ],
            0,
        );
        x_proj_flat = x_proj_flat + b_x.unsqueeze_dims::<2>(&[0]);
    }
    let x_proj = x_proj_flat.reshape([seq, batch, 4 * hidden]);

    let w_h = Tensor::cat(
        vec![
            lstm.input_gate.hidden_transform.weight.val(),
            lstm.forget_gate.hidden_transform.weight.val(),
            lstm.cell_gate.hidden_transform.weight.val(),
            lstm.output_gate.hidden_transform.weight.val(),
        ],
        1,
    );
    let b_h = Tensor::cat(
        vec![
            lstm.input_gate
                .hidden_transform
                .bias
                .as_ref()
                .unwrap()
                .val(),
            lstm.forget_gate
                .hidden_transform
                .bias
                .as_ref()
                .unwrap()
                .val(),
            lstm.cell_gate.hidden_transform.bias.as_ref().unwrap().val(),
            lstm.output_gate
                .hidden_transform
                .bias
                .as_ref()
                .unwrap()
                .val(),
        ],
        0,
    );

    let (c, h) = match state {
        Some(s) => (s.cell, s.hidden),
        None => (
            Tensor::zeros([batch, hidden], (&device, dtype)),
            Tensor::zeros([batch, hidden], (&device, dtype)),
        ),
    };
    let h_next = Tensor::<B, 2>::empty([batch, hidden], (&device, dtype));
    let c_next = Tensor::<B, 2>::empty([batch, hidden], (&device, dtype));
    let output = Tensor::<B, 3>::empty([seq, batch, hidden], (&device, dtype));

    let x_proj = cube_tensor(x_proj);
    let w_h = cube_tensor(w_h);
    let b_h = cube_tensor(b_h);
    let mut h_prev = cube_tensor(h);
    let mut c_prev = cube_tensor(c);
    let mut h_next = cube_tensor(h_next);
    let mut c_next = cube_tensor(c_next);
    let output = cube_tensor(output);

    for t in 0..seq {
        launch_lstm_step_f32(
            &x_proj, &w_h, &b_h, &h_prev, &c_prev, &h_next, &c_next, &output, t, batch, hidden,
        );
        core::mem::swap(&mut h_prev, &mut h_next);
        core::mem::swap(&mut c_prev, &mut c_next);
    }

    let output = burn_tensor(output);
    let hidden = burn_tensor(h_prev);
    let cell = burn_tensor(c_prev);

    (output, LstmState::new(cell, hidden))
}

pub(crate) fn lstm_preproj_equivalence_error<B: CustomLstmBackend>(device: &B::Device) -> f32 {
    let seq = 5;
    let batch = 3;
    let input_size = 16;
    let hidden = 32;

    let lstm = LstmConfig::new(input_size, hidden, true)
        .with_batch_first(false)
        .with_input_forget(false)
        .init(device);
    let input = Tensor::<B, 3>::ones([seq, batch, input_size], device);
    let cell = Tensor::<B, 2>::ones([batch, hidden], device) * 0.125;
    let hidden_state = Tensor::<B, 2>::ones([batch, hidden], device) * -0.25;

    let (expected_output, expected_state) = lstm.forward(
        input.clone(),
        Some(LstmState::new(cell.clone(), hidden_state.clone())),
    );
    let (actual_output, actual_state) =
        lstm_preproj(&lstm, input, Some(LstmState::new(cell, hidden_state)));

    fn max_abs_diff<B: Backend, const D: usize>(lhs: Tensor<B, D>, rhs: Tensor<B, D>) -> f32 {
        let lhs = lhs.into_data().iter::<f32>().collect::<Vec<_>>();
        let rhs = rhs.into_data().iter::<f32>().collect::<Vec<_>>();
        lhs.iter()
            .zip(rhs.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max)
    }

    max_abs_diff(expected_output, actual_output)
        .max(max_abs_diff(expected_state.cell, actual_state.cell))
        .max(max_abs_diff(expected_state.hidden, actual_state.hidden))
}
