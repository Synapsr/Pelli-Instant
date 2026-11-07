//! ONNX Runtime environment, execution-provider selection and a small
//! session pool.
//!
//! `ort` 2.0's `Session::run` takes `&mut self` (some ONNX Runtime internals
//! are not thread-safe), so concurrent `analyze` calls check a session out of
//! a [`SessionPool`]. Sessions are created on demand, up to `max` per model,
//! and kept for the life of the pool; [`SessionPool::warm`] creates them in
//! advance, in parallel, so an analysis does not pay for them (0.1 s per
//! worker and model on an M4).

use crate::{MlError, Result};
use ort::environment::Environment;
use ort::ep::ExecutionProviderDispatch;
use ort::session::builder::{GraphOptimizationLevel, PrepackedWeights, SessionBuilder};
use ort::session::Session;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

/// Execution provider a session runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    Cpu,
    CoreMl,
    DirectMl,
    Cuda,
}

impl Provider {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Provider::Cpu => "CPU",
            Provider::CoreMl => "CoreML",
            Provider::DirectMl => "DirectML",
            Provider::Cuda => "CUDA",
        }
    }

    fn dispatch(self, cache_dir: Option<&std::path::Path>, units: CoreMlUnits) -> Option<ExecutionProviderDispatch> {
        match self {
            Provider::Cpu => None,
            Provider::CoreMl => coreml_dispatch(cache_dir, units),
            Provider::DirectMl => directml_dispatch(),
            Provider::Cuda => cuda_dispatch(),
        }
    }
}

/// CoreML compute units of one model. Chosen **per model**: in fp16 the
/// Neural Engine silently returns wrong results for some graphs (DINOv2,
/// RF-DETR and CLIP ViT-L/14: cosine −0.03 to the CPU, no error, no NaN —
/// a few very large activations overflow), while the GPU is exact; other
/// models are fine with CoreML's choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CoreMlUnits {
    /// Neural Engine, GPU and CPU, CoreML's choice (small, well-behaved graphs).
    #[default]
    All,
    /// GPU and CPU only: never the Neural Engine.
    CpuAndGpu,
}

/// CoreML compiles each model for the device when a session is created
/// ("can take minutes for complex models"). With a cache directory the
/// compiled model is reused at the next launch (ONNX Runtime keys it on the
/// model; a changed model gets a new entry).
#[cfg(all(feature = "coreml", target_vendor = "apple"))]
fn coreml_dispatch(cache_dir: Option<&std::path::Path>, units: CoreMlUnits) -> Option<ExecutionProviderDispatch> {
    use ort::ep::coreml::{ComputeUnits, ModelFormat};
    let units = match units {
        CoreMlUnits::All => ComputeUnits::All,
        CoreMlUnits::CpuAndGpu => ComputeUnits::CPUAndGPU,
    };
    let mut ep = ort::ep::CoreML::default().with_model_format(ModelFormat::MLProgram).with_compute_units(units);
    if let Some(dir) = cache_dir {
        if std::fs::create_dir_all(dir).is_ok() {
            ep = ep.with_model_cache_dir(dir.display());
        }
    }
    Some(ep.build().error_on_failure())
}
#[cfg(not(all(feature = "coreml", target_vendor = "apple")))]
fn coreml_dispatch(_cache_dir: Option<&std::path::Path>, _units: CoreMlUnits) -> Option<ExecutionProviderDispatch> {
    None
}

#[cfg(all(feature = "directml", windows))]
fn directml_dispatch() -> Option<ExecutionProviderDispatch> {
    Some(ort::ep::DirectML::default().build().error_on_failure())
}
#[cfg(not(all(feature = "directml", windows)))]
fn directml_dispatch() -> Option<ExecutionProviderDispatch> {
    None
}

#[cfg(all(feature = "cuda", any(target_os = "linux", windows)))]
fn cuda_dispatch() -> Option<ExecutionProviderDispatch> {
    Some(ort::ep::CUDA::default().build().error_on_failure())
}
#[cfg(not(all(feature = "cuda", any(target_os = "linux", windows))))]
fn cuda_dispatch() -> Option<ExecutionProviderDispatch> {
    None
}

/// Accelerated providers compiled into this build for the current target, in
/// order of preference. Empty on a plain CPU build.
pub(crate) fn compiled_accelerators() -> Vec<Provider> {
    [
        (cfg!(all(feature = "cuda", any(target_os = "linux", windows))), Provider::Cuda),
        (cfg!(all(feature = "coreml", target_vendor = "apple")), Provider::CoreMl),
        (cfg!(all(feature = "directml", windows)), Provider::DirectMl),
    ]
    .into_iter()
    .filter_map(|(on, p)| on.then_some(p))
    .collect()
}

static ENVIRONMENT: OnceLock<std::result::Result<Environment, String>> = OnceLock::new();

/// The process-wide ONNX Runtime environment. `ort` allows a single
/// environment per process, so it is created once and never dropped.
fn environment() -> Result<&'static Environment> {
    ENVIRONMENT
        .get_or_init(|| ort::init().with_name("pelli-cull").with_telemetry(false).build().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| MlError::Runtime(format!("cannot initialise ONNX Runtime: {e}")))
}

fn rt<T, R>(r: std::result::Result<T, ort::Error<R>>) -> std::result::Result<T, String> {
    r.map_err(|e| e.to_string())
}

/// Where the sessions of a model are built from.
#[derive(Clone)]
pub(crate) enum ModelSource {
    /// Verified file on disk (large models: not kept in memory).
    File(PathBuf),
    /// Model bytes kept in memory (small or patched models).
    Memory(Arc<[u8]>),
}

/// How to build the sessions of one model.
#[derive(Clone)]
pub(crate) struct SessionSpec {
    pub label: &'static str,
    pub source: ModelSource,
    pub intra_threads: usize,
    pub provider: Provider,
    /// Where an accelerator keeps its compiled models (CoreML).
    pub cache_dir: Option<PathBuf>,
    pub coreml_units: CoreMlUnits,
    /// Symbolic input dimensions fixed at session creation (`batch_size`
    /// = 1, `height` = 224…): ONNX Runtime then folds the shape-dependent
    /// parts of the graph into constants, and CoreML takes the whole graph
    /// in one partition instead of many.
    pub dims: &'static [(&'static str, i64)],
}

/// Options of [`build_first`] for one model.
#[derive(Clone, Copy)]
pub(crate) struct ModelOpts<'a> {
    pub intra_threads: usize,
    /// Accelerated providers to try first, in order.
    pub candidates: &'a [Provider],
    /// Fall back to the CPU when no candidate works.
    pub cpu_fallback: bool,
    pub prepacked: Option<&'a PrepackedWeights>,
    pub cache_dir: Option<&'a std::path::Path>,
    pub coreml_units: CoreMlUnits,
    pub dims: &'static [(&'static str, i64)],
}

impl<'a> ModelOpts<'a> {
    /// CPU only, `intra_threads` threads.
    pub(crate) fn cpu(intra_threads: usize) -> ModelOpts<'a> {
        ModelOpts {
            intra_threads,
            candidates: &[],
            cpu_fallback: true,
            prepacked: None,
            cache_dir: None,
            coreml_units: CoreMlUnits::All,
            dims: &[],
        }
    }
}

fn builder(spec: &SessionSpec, prepacked: Option<&PrepackedWeights>) -> std::result::Result<SessionBuilder, String> {
    let env = environment().map_err(|e| e.to_string())?;
    let mut b = rt(Session::builder(env))?;
    b = rt(b.with_optimization_level(GraphOptimizationLevel::Level3))?;
    let intra = spec.intra_threads.max(1);
    b = rt(b.with_intra_threads(intra))?;
    b = rt(b.with_inter_threads(1))?;
    if intra > 1 {
        // The engine runs other CPU work next to us: don't burn cores spinning.
        b = rt(b.with_intra_op_spinning(false))?;
    }
    if spec.provider == Provider::DirectMl {
        // Required by the DirectML execution provider.
        b = rt(b.with_memory_pattern(false))?;
        b = rt(b.with_parallel_execution(false))?;
    }
    if let Some(p) = prepacked {
        b = rt(b.with_prepacked_weights(p))?;
    }
    for (name, size) in spec.dims {
        b = rt(b.with_dimension_override(name, *size))?;
    }
    if let Some(ep) = spec.provider.dispatch(spec.cache_dir.as_deref(), spec.coreml_units) {
        b = rt(b.with_execution_providers([ep]))?;
    }
    Ok(b)
}

/// Build one session from `bytes` (already checksum-verified) or, when
/// `None`, from `spec.source`.
fn build(
    spec: &SessionSpec,
    bytes: Option<&[u8]>,
    prepacked: Option<&PrepackedWeights>,
) -> std::result::Result<Session, String> {
    let mut b = builder(spec, prepacked)?;
    match (bytes, &spec.source) {
        (Some(bytes), _) => rt(b.commit_from_memory(bytes)),
        (None, ModelSource::Memory(bytes)) => rt(b.commit_from_memory(bytes)),
        (None, ModelSource::File(path)) => rt(b.commit_from_file(path)),
    }
}

/// Build the first session of a model from its verified `bytes` (or from
/// `source` when `None`: large files are not read into memory), trying the
/// `candidates` providers in order, then the CPU when `cpu_fallback`
/// (silently, with a log line). Returns the spec that worked.
pub(crate) fn build_first(
    label: &'static str,
    source: ModelSource,
    bytes: Option<&[u8]>,
    o: &ModelOpts<'_>,
) -> Result<(Session, SessionSpec)> {
    let mut last_err = String::from("no execution provider");
    let mut tried_cpu = false;
    let cpu: &[Provider] = if o.cpu_fallback { &[Provider::Cpu] } else { &[] };
    for &provider in o.candidates.iter().chain(cpu.iter()) {
        if provider == Provider::Cpu {
            if tried_cpu {
                continue;
            }
            tried_cpu = true;
        }
        let cache_dir = (provider != Provider::Cpu).then(|| o.cache_dir.map(|d| d.to_path_buf())).flatten();
        let spec = SessionSpec {
            label,
            source: source.clone(),
            intra_threads: o.intra_threads,
            provider,
            cache_dir,
            coreml_units: o.coreml_units,
            dims: o.dims,
        };
        // Prepacked weights only make sense for CPU kernels.
        let pp = if provider == Provider::Cpu { o.prepacked } else { None };
        match build(&spec, bytes, pp) {
            Ok(s) => {
                log::info!("cull-ml: {label} on {}", provider.name());
                return Ok((s, spec));
            }
            Err(e) => {
                log::warn!("cull-ml: {label}: {} unavailable ({e}), falling back", provider.name());
                last_err = e;
            }
        }
    }
    Err(MlError::Runtime(format!("{label}: {last_err}")))
}

struct PoolState {
    idle: Vec<Session>,
    total: usize,
}

/// Lazily grown pool of identical sessions of one model.
pub(crate) struct SessionPool {
    spec: SessionSpec,
    prepacked: Option<PrepackedWeights>,
    max: usize,
    state: Mutex<PoolState>,
    available: Condvar,
    /// Held while [`SessionPool::warm`] builds: a second warm-up waits for
    /// the sessions to exist instead of returning early.
    warming: Mutex<()>,
}

impl SessionPool {
    pub(crate) fn new(
        first: Session,
        spec: SessionSpec,
        prepacked: Option<PrepackedWeights>,
        max: usize,
    ) -> SessionPool {
        SessionPool {
            spec,
            prepacked,
            max: max.max(1),
            state: Mutex::new(PoolState { idle: vec![first], total: 1 }),
            available: Condvar::new(),
            warming: Mutex::new(()),
        }
    }

    pub(crate) fn provider(&self) -> Provider {
        self.spec.provider
    }

    fn lock(&self) -> MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn checkout(&self) -> Result<Session> {
        let mut st = self.lock();
        loop {
            if let Some(s) = st.idle.pop() {
                return Ok(s);
            }
            if st.total < self.max {
                st.total += 1;
                drop(st);
                let pp = if self.spec.provider == Provider::Cpu { self.prepacked.as_ref() } else { None };
                return match build(&self.spec, None, pp) {
                    Ok(s) => Ok(s),
                    Err(e) => {
                        let mut st = self.lock();
                        st.total -= 1;
                        self.available.notify_one();
                        Err(MlError::Runtime(format!("{}: {e}", self.spec.label)))
                    }
                };
            }
            st = self.available.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// Create sessions until the pool holds `n` (at most its maximum), in
    /// parallel. Failures are logged and leave the pool smaller: `with` grows
    /// it again on demand.
    pub(crate) fn warm(&self, n: usize) {
        let _one_at_a_time = self.warming.lock().unwrap_or_else(|e| e.into_inner());
        let missing = {
            let mut st = self.lock();
            let m = n.min(self.max).saturating_sub(st.total);
            st.total += m;
            m
        };
        if missing == 0 {
            return;
        }
        let pp = if self.spec.provider == Provider::Cpu { self.prepacked.as_ref() } else { None };
        let built: Vec<std::result::Result<Session, String>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..missing).map(|_| s.spawn(|| build(&self.spec, None, pp))).collect();
            handles.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("panic".into()))).collect()
        });
        let mut st = self.lock();
        for r in built {
            match r {
                Ok(session) => st.idle.push(session),
                Err(e) => {
                    st.total -= 1;
                    log::warn!("cull-ml: {}: {e}", self.spec.label);
                }
            }
        }
        drop(st);
        self.available.notify_all();
    }

    fn checkin(&self, s: Session) {
        let mut st = self.lock();
        st.idle.push(s);
        drop(st);
        self.available.notify_one();
    }

    /// Run `f` with exclusive access to one session of the pool.
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut Session) -> Result<R>) -> Result<R> {
        struct Guard<'a> {
            pool: &'a SessionPool,
            session: Option<Session>,
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                if let Some(s) = self.session.take() {
                    self.pool.checkin(s);
                }
            }
        }
        let mut guard = Guard { pool: self, session: Some(self.checkout()?) };
        match guard.session.as_mut() {
            Some(s) => f(s),
            None => Err(MlError::Runtime("session pool: no session".into())),
        }
    }
}

/// Output `name` of a run as `f32`s, checking it holds at least `min_len`
/// values.
pub(crate) fn output_f32<'a>(
    outputs: &'a ort::session::SessionOutputs<'_>,
    name: &str,
    min_len: usize,
) -> Result<&'a [f32]> {
    let v = outputs.get(name).ok_or_else(|| MlError::Runtime(format!("missing model output {name}")))?;
    let (_, data) = v.try_extract_tensor::<f32>().map_err(|e| MlError::Runtime(format!("model output {name}: {e}")))?;
    if data.len() < min_len {
        return Err(MlError::Runtime(format!("model output {name}: {} values, expected {min_len}", data.len())));
    }
    Ok(data)
}

/// Default upper bound of concurrent sessions per CPU model.
pub(crate) fn default_pool_size() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 32)
}
