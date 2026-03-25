// Types for the contract interaction interface

export type ParamKind = 'address' | 'amount' | 'string' | 'u32' | 'i128' | 'bool' | 'bytes';

export interface ParamDef {
  name: string;
  kind: ParamKind;
  label: string;
  placeholder?: string;
  hint?: string;
  required?: boolean;
}

export interface FunctionDef {
  name: string;       // e.g. "transfer"
  label: string;
  icon: string;
  description: string;
  docs?: string;
  estimatedFee: string; // XLM
  params: ParamDef[];
  readOnly?: boolean;   // view functions — no tx needed
}

export interface SimulationResult {
  success: boolean;
  returnValue?: string;
  gasUsed?: number;
  feeXLM?: string;
  error?: string;
}

export interface BatchItem {
  id: string;
  fn: FunctionDef;
  params: Record<string, string>;
  status: 'queued' | 'submitted' | 'done' | 'error';
  error?: string;
}

export interface ContractABI {
  contractId: string;
  name: string;
  functions: FunctionDef[];
}
