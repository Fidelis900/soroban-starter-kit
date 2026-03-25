import type { ContractABI, FunctionDef, SimulationResult, ParamDef } from './types';

// ── Built-in ABIs ─────────────────────────────────────────────────────────────

const TOKEN_ABI: FunctionDef[] = [
  {
    name: 'transfer', label: 'Transfer', icon: '💸',
    description: 'Send tokens to another address.',
    docs: 'Transfers `amount` stroops from the caller to `to`. Emits a Transfer event.',
    estimatedFee: '0.00001',
    params: [
      { name: 'to',     kind: 'address', label: 'Recipient',      placeholder: 'G...', hint: 'Stellar public key', required: true },
      { name: 'amount', kind: 'amount',  label: 'Amount (stroops)', placeholder: '10000000', hint: '1 token = 10,000,000 stroops', required: true },
    ],
  },
  {
    name: 'mint', label: 'Mint', icon: '🪙',
    description: 'Create new tokens (admin only).',
    docs: 'Mints `amount` stroops to `to`. Caller must be the contract admin.',
    estimatedFee: '0.00001',
    params: [
      { name: 'to',     kind: 'address', label: 'Recipient',      placeholder: 'G...', required: true },
      { name: 'amount', kind: 'amount',  label: 'Amount (stroops)', placeholder: '10000000', required: true },
    ],
  },
  {
    name: 'burn', label: 'Burn', icon: '🔥',
    description: 'Destroy tokens from your balance.',
    docs: 'Burns `amount` stroops from the caller\'s balance.',
    estimatedFee: '0.00001',
    params: [
      { name: 'amount', kind: 'amount', label: 'Amount (stroops)', placeholder: '10000000', required: true },
    ],
  },
  {
    name: 'approve', label: 'Approve', icon: '✅',
    description: 'Allow a spender to use tokens on your behalf.',
    docs: 'Sets the allowance for `spender` to `amount` stroops, expiring at `expiration_ledger`.',
    estimatedFee: '0.00001',
    params: [
      { name: 'spender',           kind: 'address', label: 'Spender Address',    placeholder: 'G...', required: true },
      { name: 'amount',            kind: 'amount',  label: 'Allowance (stroops)', placeholder: '10000000', required: true },
      { name: 'expiration_ledger', kind: 'u32',     label: 'Expiration Ledger',   placeholder: '1000000', hint: 'Ledger number when approval expires', required: true },
    ],
  },
  {
    name: 'balance', label: 'Balance', icon: '📊',
    description: 'Query token balance of an address.',
    docs: 'Returns the balance in stroops for `id`. Read-only — no transaction required.',
    estimatedFee: '0',
    readOnly: true,
    params: [
      { name: 'id', kind: 'address', label: 'Address', placeholder: 'G...', required: true },
    ],
  },
];

const ESCROW_ABI: FunctionDef[] = [
  {
    name: 'fund', label: 'Fund Escrow', icon: '🔒',
    description: 'Deposit tokens into an escrow contract.',
    docs: 'Transfers `amount` stroops into the escrow. Caller must be the buyer.',
    estimatedFee: '0.00002',
    params: [
      { name: 'escrowId', kind: 'string', label: 'Escrow Contract ID', placeholder: 'C...', required: true },
      { name: 'amount',   kind: 'amount', label: 'Amount (stroops)',    placeholder: '10000000', required: true },
    ],
  },
  {
    name: 'release', label: 'Release Escrow', icon: '🔓',
    description: 'Release escrowed funds to the seller.',
    docs: 'Marks delivery complete and releases funds to the seller. Caller must be buyer or arbiter.',
    estimatedFee: '0.00001',
    params: [
      { name: 'escrowId', kind: 'string', label: 'Escrow Contract ID', placeholder: 'C...', required: true },
    ],
  },
  {
    name: 'refund', label: 'Refund Escrow', icon: '↩️',
    description: 'Refund escrowed funds back to the buyer.',
    docs: 'Returns funds to the buyer. Only callable after deadline or by arbiter.',
    estimatedFee: '0.00001',
    params: [
      { name: 'escrowId', kind: 'string', label: 'Escrow Contract ID', placeholder: 'C...', required: true },
    ],
  },
];

const BUILT_IN_ABIS: ContractABI[] = [
  { contractId: 'token',  name: 'Token Contract',  functions: TOKEN_ABI  },
  { contractId: 'escrow', name: 'Escrow Contract', functions: ESCROW_ABI },
];

// ── Validation ────────────────────────────────────────────────────────────────

export function validateParam(value: string, def: ParamDef): string | null {
  if (def.required && !value.trim()) return `${def.label} is required.`;
  if (!value.trim()) return null;
  switch (def.kind) {
    case 'address':
      if (!/^[GC][A-Z2-7]{55}$/.test(value.trim())) return 'Must be a valid Stellar/contract address.';
      break;
    case 'amount':
    case 'u32': {
      const n = Number(value);
      if (!Number.isInteger(n) || n <= 0) return 'Must be a positive integer.';
      break;
    }
    case 'i128': {
      if (!/^-?\d+$/.test(value.trim())) return 'Must be an integer.';
      break;
    }
    case 'bool':
      if (!['true', 'false'].includes(value.trim().toLowerCase())) return 'Must be true or false.';
      break;
  }
  return null;
}

// ── Simulation (mock — replace with real Soroban RPC call) ───────────────────

export async function simulateTransaction(
  contractId: string,
  fnName: string,
  params: Record<string, string>,
): Promise<SimulationResult> {
  // Simulate network latency
  await new Promise((r) => setTimeout(r, 600));

  // Mock: fail if contractId looks invalid
  if (contractId && !/^[A-Za-z0-9_-]+$/.test(contractId)) {
    return { success: false, error: 'Invalid contract ID format.' };
  }

  const gasUsed = 500_000 + Math.floor(Math.random() * 200_000);
  const feeXLM = (gasUsed * 0.0000001).toFixed(7);
  return {
    success: true,
    returnValue: fnName === 'balance' ? String(Math.floor(Math.random() * 1e9)) : undefined,
    gasUsed,
    feeXLM,
  };
}

// ── ABI discovery ─────────────────────────────────────────────────────────────

export function getBuiltInABIs(): ContractABI[] {
  return BUILT_IN_ABIS;
}

/** Parse a minimal JSON ABI (array of FunctionDef) pasted by the user */
export function parseCustomABI(json: string): ContractABI | null {
  try {
    const parsed = JSON.parse(json);
    if (!Array.isArray(parsed)) return null;
    const functions: FunctionDef[] = parsed.map((f: Record<string, unknown>) => ({
      name:         String(f.name ?? ''),
      label:        String(f.label ?? f.name ?? ''),
      icon:         String(f.icon ?? '⚙️'),
      description:  String(f.description ?? ''),
      docs:         f.docs ? String(f.docs) : undefined,
      estimatedFee: String(f.estimatedFee ?? '0.00001'),
      readOnly:     Boolean(f.readOnly),
      params:       Array.isArray(f.params) ? f.params.map((p: Record<string, unknown>) => ({
        name:        String(p.name ?? ''),
        kind:        String(p.kind ?? 'string') as ParamDef['kind'],
        label:       String(p.label ?? p.name ?? ''),
        placeholder: p.placeholder ? String(p.placeholder) : undefined,
        hint:        p.hint ? String(p.hint) : undefined,
        required:    Boolean(p.required),
      })) : [],
    }));
    return { contractId: 'custom', name: 'Custom Contract', functions };
  } catch {
    return null;
  }
}
