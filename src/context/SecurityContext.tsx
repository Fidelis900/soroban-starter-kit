import React, { createContext, useContext, useEffect, useState, ReactNode } from 'react';
import { securityService } from '../services/security';
import type { SecurityState, AuthMethod, SecurityConfig } from '../services/security';

interface SecurityContextType extends SecurityState {
  authenticateBiometric: () => Promise<boolean>;
  generateTOTPSecret: () => string;
  verifyTOTP: (secret: string, code: string) => Promise<boolean>;
  createSession: (methods: AuthMethod[]) => Promise<void>;
  endSession: () => void;
  touchSession: () => void;
  isSessionValid: () => boolean;
  dismissAlert: (id: string) => void;
  updateConfig: (patch: Partial<SecurityConfig>) => void;
  unlock: () => void;
  encryptData: (data: string, passphrase: string) => Promise<string>;
  decryptData: (encoded: string, passphrase: string) => Promise<string>;
}

const SecurityContext = createContext<SecurityContextType | undefined>(undefined);

export function SecurityProvider({ children }: { children: ReactNode }): JSX.Element {
  const [state, setState] = useState<SecurityState>(securityService.getState());

  useEffect(() => {
    securityService.init().then(() => setState(securityService.getState()));
    return securityService.subscribe(setState);
  }, []);

  const value: SecurityContextType = {
    ...state,
    authenticateBiometric: () => securityService.authenticateBiometric(),
    generateTOTPSecret: () => securityService.generateTOTPSecret(),
    verifyTOTP: (s, c) => securityService.verifyTOTP(s, c),
    createSession: async (methods) => { await securityService.createSession(methods); },
    endSession: () => securityService.endSession(),
    touchSession: () => securityService.touchSession(),
    isSessionValid: () => securityService.isSessionValid(),
    dismissAlert: (id) => securityService.dismissAlert(id),
    updateConfig: (patch) => securityService.updateConfig(patch),
    unlock: () => securityService.unlock(),
    encryptData: (d, p) => securityService.encryptSensitiveData(d, p),
    decryptData: (e, p) => securityService.decryptSensitiveData(e, p),
  };

  return <SecurityContext.Provider value={value}>{children}</SecurityContext.Provider>;
}

export function useSecurity(): SecurityContextType {
  const ctx = useContext(SecurityContext);
  if (!ctx) throw new Error('useSecurity must be used within a SecurityProvider');
  return ctx;
}

export default SecurityContext;
