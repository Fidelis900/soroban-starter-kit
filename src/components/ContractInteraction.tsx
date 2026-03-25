import React, { useState, useCallback, useMemo } from 'react';
import {
  getBuiltInABIs, parseCustomABI, validateParam, simulateTransaction,
} from '../services/contractInteraction';
import type { ContractABI, FunctionDef, SimulationResult, BatchItem, ParamDef } from '../services/contractInteraction';
import { useTransactionQueue } from '../context/TransactionQueueContext';
import type { TransactionType } from '../services/storage/types';

// ── Helpers ──────────────────────────────────────────────────────────────────

function uid() { return `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`; }

const STATUS_COLOR: Record<BatchItem['status'], string> = {
  queued: '#f59e0b', submitted: '#6366f1', done: '#22c55e', error: '#ef4444',
};

// ── Main component ───────────────────────────────────────────────────────────

export function ContractInteraction(): JSX.Element {
  const { createTransaction } = useTransactionQueue();

  // ABI selection
  const builtIn = useMemo(() => getBuiltInABIs(), []);
  const [selectedABI, setSelectedABI] = useState<ContractABI>(builtIn[0]);
  const [customJSON, setCustomJSON] = useState('');
  const [customError, setCustomError] = useState('');
  const [showCustom, setShowCustom] = useState(false);

  // Contract ID override
  const [contractId, setContractId] = useState('');

  // Selected function
  const [selectedFn, setSelectedFn] = useState<FunctionDef>(builtIn[0].functions[0]);

  // Param values + errors
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [paramErrors, setParamErrors] = useState<Record<string, string>>({});

  // Simulation
  const [simResult, setSimResult] = useState<SimulationResult | null>(null);
  const [simLoading, setSimLoading] = useState(false);

  // Batch queue
  const [batch, setBatch] = useState<BatchItem[]>([]);
  const [batchRunning, setBatchRunning] = useState(false);

  // Docs panel
  const [showDocs, setShowDocs] = useState(false);

  // ── Handlers ────────────────────────────────────────────────────────────────

  const loadCustomABI = () => {
    const abi = parseCustomABI(customJSON);
    if (!abi) { setCustomError('Invalid ABI JSON. Expected an array of FunctionDef objects.'); return; }
    setCustomError('');
    setSelectedABI(abi);
    setSelectedFn(abi.functions[0]);
    setParamValues({});
    setSimResult(null);
    setShowCustom(false);
  };

  const selectFn = (fn: FunctionDef) => {
    setSelectedFn(fn);
    setParamValues({});
    setParamErrors({});
    setSimResult(null);
  };

  const setParam = (name: string, value: string) => {
    setParamValues((v) => ({ ...v, [name]: value }));
    setParamErrors((e) => ({ ...e, [name]: '' }));
  };

  const validate = (): boolean => {
    const errors: Record<string, string> = {};
    for (const p of selectedFn.params) {
      const err = validateParam(paramValues[p.name] ?? '', p);
      if (err) errors[p.name] = err;
    }
    setParamErrors(errors);
    return Object.keys(errors).length === 0;
  };

  const handleSimulate = async () => {
    if (!validate()) return;
    setSimLoading(true);
    setSimResult(null);
    const result = await simulateTransaction(
      contractId || selectedABI.contractId,
      selectedFn.name,
      paramValues,
    );
    setSimResult(result);
    setSimLoading(false);
  };

  const handleSubmit = async () => {
    if (!validate()) return;
    await createTransaction(
      selectedFn.name as TransactionType,
      contractId || selectedABI.contractId,
      selectedFn.name,
      { ...paramValues },
    );
    setParamValues({});
    setSimResult(null);
  };

  const addToBatch = () => {
    if (!validate()) return;
    setBatch((b) => [...b, {
      id: uid(),
      fn: selectedFn,
      params: { ...paramValues },
      status: 'queued',
    }]);
    setParamValues({});
    setSimResult(null);
  };

  const removeBatchItem = (id: string) => setBatch((b) => b.filter((i) => i.id !== id));

  const runBatch = async () => {
    if (!batch.length) return;
    setBatchRunning(true);
    for (const item of batch) {
      setBatch((b) => b.map((i) => i.id === item.id ? { ...i, status: 'submitted' } : i));
      try {
        await createTransaction(
          item.fn.name as TransactionType,
          contractId || selectedABI.contractId,
          item.fn.name,
          { ...item.params },
        );
        setBatch((b) => b.map((i) => i.id === item.id ? { ...i, status: 'done' } : i));
      } catch (e) {
        setBatch((b) => b.map((i) => i.id === item.id ? { ...i, status: 'error', error: String(e) } : i));
      }
    }
    setBatchRunning(false);
  };

  // ── Render ───────────────────────────────────────────────────────────────────

  return (
    <div style={{ fontFamily: 'sans-serif', maxWidth: 1000, margin: '0 auto', padding: 16 }}>
      <h2 style={{ margin: '0 0 16px' }}>Contract Interaction</h2>

      <div style={{ display: 'grid', gridTemplateColumns: '220px 1fr', gap: 16, alignItems: 'start' }}>

        {/* ── Left: ABI + function list ── */}
        <aside>
          {/* ABI selector */}
          <div style={card}>
            <label style={labelStyle}>
              Contract ABI
              <select
                value={selectedABI.contractId}
                onChange={(e) => {
                  const abi = builtIn.find((a) => a.contractId === e.target.value);
                  if (abi) { setSelectedABI(abi); selectFn(abi.functions[0]); }
                }}
                style={inputStyle}
              >
                {builtIn.map((a) => <option key={a.contractId} value={a.contractId}>{a.name}</option>)}
              </select>
            </label>
            <button onClick={() => setShowCustom((v) => !v)} style={linkBtn}>
              {showCustom ? '▲ Hide' : '+ Custom ABI'}
            </button>
            {showCustom && (
              <div style={{ marginTop: 8 }}>
                <textarea
                  rows={5}
                  placeholder='[{"name":"transfer","label":"Transfer","icon":"💸","estimatedFee":"0.00001","params":[...]}]'
                  value={customJSON}
                  onChange={(e) => setCustomJSON(e.target.value)}
                  style={{ ...inputStyle, resize: 'vertical', fontFamily: 'monospace', fontSize: 11 }}
                />
                {customError && <p style={errorStyle}>{customError}</p>}
                <button onClick={loadCustomABI} style={primaryBtn}>Load ABI</button>
              </div>
            )}
          </div>

          {/* Contract ID */}
          <div style={{ ...card, marginTop: 8 }}>
            <label style={labelStyle}>
              Contract ID (optional)
              <input
                placeholder={selectedABI.contractId}
                value={contractId}
                onChange={(e) => setContractId(e.target.value)}
                style={inputStyle}
              />
            </label>
          </div>

          {/* Function list */}
          <nav aria-label="Contract functions" style={{ ...card, marginTop: 8, padding: 0, overflow: 'hidden' }}>
            {selectedABI.functions.map((fn) => (
              <button
                key={fn.name}
                onClick={() => selectFn(fn)}
                aria-current={selectedFn.name === fn.name}
                style={{
                  display: 'flex', alignItems: 'center', gap: 8,
                  width: '100%', padding: '10px 14px', border: 'none', cursor: 'pointer', textAlign: 'left',
                  background: selectedFn.name === fn.name ? '#ede9fe' : 'transparent',
                  borderLeft: selectedFn.name === fn.name ? '3px solid #6366f1' : '3px solid transparent',
                  fontSize: 13,
                }}
              >
                <span aria-hidden>{fn.icon}</span>
                <span style={{ flex: 1 }}>{fn.label}</span>
                {fn.readOnly && <span style={{ fontSize: 10, color: '#6b7280', background: '#f3f4f6', padding: '1px 5px', borderRadius: 4 }}>view</span>}
              </button>
            ))}
          </nav>
        </aside>

        {/* ── Right: function form ── */}
        <main>
          <div style={card}>
            {/* Function header */}
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', marginBottom: 12 }}>
              <div>
                <h3 style={{ margin: 0, fontSize: 16 }}>{selectedFn.icon} {selectedFn.label}</h3>
                <p style={{ margin: '4px 0 0', fontSize: 13, color: '#6b7280' }}>{selectedFn.description}</p>
              </div>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                <span style={{ fontSize: 12, color: '#6b7280' }}>
                  Est. fee: <strong>{selectedFn.estimatedFee} XLM</strong>
                </span>
                {selectedFn.docs && (
                  <button onClick={() => setShowDocs((v) => !v)} style={linkBtn}>
                    {showDocs ? 'Hide docs' : '📖 Docs'}
                  </button>
                )}
              </div>
            </div>

            {/* Docs */}
            {showDocs && selectedFn.docs && (
              <div style={{ background: '#f0fdf4', border: '1px solid #bbf7d0', borderRadius: 6, padding: 10, fontSize: 13, marginBottom: 12 }}>
                {selectedFn.docs}
              </div>
            )}

            {/* Params */}
            {selectedFn.params.length === 0 ? (
              <p style={{ color: '#9ca3af', fontSize: 13 }}>No parameters required.</p>
            ) : (
              selectedFn.params.map((p) => (
                <ParamInput
                  key={p.name}
                  def={p}
                  value={paramValues[p.name] ?? ''}
                  error={paramErrors[p.name]}
                  onChange={(v) => setParam(p.name, v)}
                />
              ))
            )}

            {/* Simulation result */}
            {simResult && <SimulationPanel result={simResult} />}

            {/* Actions */}
            <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 16 }}>
              <button onClick={handleSimulate} disabled={simLoading} style={secondaryBtn}>
                {simLoading ? '⏳ Simulating…' : '🔍 Simulate'}
              </button>
              {!selectedFn.readOnly && (
                <>
                  <button onClick={handleSubmit} style={primaryBtn}>
                    🚀 Submit
                  </button>
                  <button onClick={addToBatch} style={secondaryBtn}>
                    + Add to Batch
                  </button>
                </>
              )}
            </div>
          </div>

          {/* ── Batch queue ── */}
          {batch.length > 0 && (
            <div style={{ ...card, marginTop: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
                <h3 style={{ margin: 0, fontSize: 15 }}>Batch Queue ({batch.length})</h3>
                <div style={{ display: 'flex', gap: 8 }}>
                  <button onClick={runBatch} disabled={batchRunning} style={primaryBtn}>
                    {batchRunning ? '⏳ Running…' : '▶ Run All'}
                  </button>
                  <button onClick={() => setBatch([])} style={secondaryBtn}>Clear</button>
                </div>
              </div>
              {batch.map((item) => (
                <div key={item.id} style={{
                  display: 'flex', alignItems: 'center', gap: 10,
                  padding: '8px 10px', borderRadius: 6, marginBottom: 6,
                  background: '#f9fafb', border: `1px solid ${STATUS_COLOR[item.status]}44`,
                }}>
                  <span style={{ fontSize: 16 }} aria-hidden>{item.fn.icon}</span>
                  <div style={{ flex: 1, fontSize: 13 }}>
                    <strong>{item.fn.label}</strong>
                    <span style={{ color: '#6b7280', marginLeft: 8 }}>
                      {Object.entries(item.params).map(([k, v]) => `${k}=${v}`).join(', ')}
                    </span>
                    {item.error && <span style={{ color: '#ef4444', marginLeft: 8 }}>⚠ {item.error}</span>}
                  </div>
                  <span style={{ fontSize: 11, fontWeight: 700, color: STATUS_COLOR[item.status] }}>
                    {item.status}
                  </span>
                  {item.status === 'queued' && (
                    <button onClick={() => removeBatchItem(item.id)} aria-label="Remove" style={{ background: 'none', border: 'none', cursor: 'pointer', color: '#9ca3af' }}>✕</button>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* ── Education ── */}
          <EducationPanel />
        </main>
      </div>
    </div>
  );
}

// ── Param input ──────────────────────────────────────────────────────────────

function ParamInput({ def, value, error, onChange }: {
  def: ParamDef; value: string; error?: string; onChange: (v: string) => void;
}) {
  return (
    <div style={{ marginBottom: 12 }}>
      <label style={{ display: 'block', fontSize: 13, fontWeight: 500, marginBottom: 4 }}>
        {def.label}
        {def.required && <span style={{ color: '#ef4444', marginLeft: 2 }}>*</span>}
        <span style={{ fontSize: 11, color: '#9ca3af', fontWeight: 400, marginLeft: 6 }}>({def.kind})</span>
      </label>
      {def.kind === 'bool' ? (
        <select value={value} onChange={(e) => onChange(e.target.value)} style={inputStyle}>
          <option value="">Select…</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      ) : (
        <input
          type={def.kind === 'amount' || def.kind === 'u32' ? 'number' : 'text'}
          placeholder={def.placeholder}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          aria-invalid={!!error}
          aria-describedby={error ? `${def.name}-err` : undefined}
          style={{ ...inputStyle, borderColor: error ? '#ef4444' : '#d1d5db' }}
        />
      )}
      {def.hint && !error && <p style={{ margin: '3px 0 0', fontSize: 11, color: '#9ca3af' }}>{def.hint}</p>}
      {error && <p id={`${def.name}-err`} role="alert" style={errorStyle}>{error}</p>}
    </div>
  );
}

// ── Simulation panel ─────────────────────────────────────────────────────────

function SimulationPanel({ result }: { result: SimulationResult }) {
  return (
    <div style={{
      marginTop: 12, padding: 12, borderRadius: 8,
      background: result.success ? '#f0fdf4' : '#fef2f2',
      border: `1px solid ${result.success ? '#bbf7d0' : '#fecaca'}`,
      fontSize: 13,
    }} role="status" aria-live="polite">
      <strong>{result.success ? '✅ Simulation succeeded' : '❌ Simulation failed'}</strong>
      {result.error && <p style={{ margin: '4px 0 0', color: '#b91c1c' }}>{result.error}</p>}
      {result.success && (
        <dl style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '4px 16px', margin: '8px 0 0' }}>
          {result.returnValue !== undefined && <><dt style={{ color: '#6b7280' }}>Return value</dt><dd style={{ margin: 0, fontWeight: 600 }}>{result.returnValue}</dd></>}
          {result.gasUsed !== undefined && <><dt style={{ color: '#6b7280' }}>Gas used</dt><dd style={{ margin: 0, fontWeight: 600 }}>{result.gasUsed.toLocaleString()}</dd></>}
          {result.feeXLM !== undefined && <><dt style={{ color: '#6b7280' }}>Estimated fee</dt><dd style={{ margin: 0, fontWeight: 600 }}>{result.feeXLM} XLM</dd></>}
        </dl>
      )}
    </div>
  );
}

// ── Education panel ──────────────────────────────────────────────────────────

const TIPS = [
  { icon: '🔍', title: 'Simulate before submitting', body: 'Always run Simulate first to catch errors and see the estimated fee before broadcasting a transaction.' },
  { icon: '⛽', title: 'Gas & fees on Soroban', body: 'Soroban charges fees based on CPU instructions and ledger entries. The estimate shown is approximate — actual fees may vary slightly.' },
  { icon: '📦', title: 'Batch operations', body: 'Use "Add to Batch" to queue multiple calls and submit them in sequence with a single click, saving time for bulk operations.' },
  { icon: '📖', title: 'Custom ABI', body: 'Paste a JSON ABI array to interact with any deployed Soroban contract, not just the built-in templates.' },
  { icon: '👁', title: 'View functions', body: 'Functions marked "view" are read-only queries — they don\'t create transactions and cost no fees.' },
];

function EducationPanel() {
  const [open, setOpen] = useState(false);
  return (
    <div style={{ ...card, marginTop: 16 }}>
      <button onClick={() => setOpen((v) => !v)} style={{ ...linkBtn, fontWeight: 600, fontSize: 14 }}>
        {open ? '▲' : '▼'} 📚 How contract interaction works
      </button>
      {open && (
        <div style={{ marginTop: 12, display: 'grid', gap: 10 }}>
          {TIPS.map((t) => (
            <div key={t.title} style={{ padding: 10, borderRadius: 6, background: '#f9fafb', border: '1px solid #e5e7eb', fontSize: 13 }}>
              <strong>{t.icon} {t.title}</strong>
              <p style={{ margin: '4px 0 0', color: '#6b7280' }}>{t.body}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Styles ───────────────────────────────────────────────────────────────────

const card: React.CSSProperties = { border: '1px solid #e5e7eb', borderRadius: 10, padding: 16 };
const inputStyle: React.CSSProperties = { width: '100%', padding: '6px 10px', borderRadius: 6, border: '1px solid #d1d5db', fontSize: 13, boxSizing: 'border-box', marginTop: 2 };
const labelStyle: React.CSSProperties = { display: 'block', fontSize: 12, color: '#6b7280', marginBottom: 4 };
const errorStyle: React.CSSProperties = { margin: '3px 0 0', fontSize: 11, color: '#ef4444' };
const primaryBtn: React.CSSProperties = { padding: '7px 16px', borderRadius: 6, border: 'none', cursor: 'pointer', background: '#6366f1', color: '#fff', fontSize: 13, fontWeight: 600 };
const secondaryBtn: React.CSSProperties = { padding: '7px 16px', borderRadius: 6, border: 'none', cursor: 'pointer', background: '#ede9fe', color: '#4f46e5', fontSize: 13, fontWeight: 500 };
const linkBtn: React.CSSProperties = { background: 'none', border: 'none', cursor: 'pointer', color: '#6366f1', fontSize: 13, padding: 0 };
