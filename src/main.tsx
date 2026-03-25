import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { ConnectivityProvider } from './context/ConnectivityContext';
import { StorageProvider } from './context/StorageContext';
import { TransactionQueueProvider } from './context/TransactionQueueContext';
import { ThemeProvider } from './context/ThemeContext';
import { TutorialProvider } from './context/TutorialContext';
import { PWAProvider } from './context/PWAContext';
import { SecurityProvider } from './context/SecurityContext';
import './styles/index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ThemeProvider>
      <TutorialProvider>
        <PWAProvider>
          <SecurityProvider>
            <ConnectivityProvider>
              <StorageProvider>
                <TransactionQueueProvider>
                  <App />
                </TransactionQueueProvider>
              </StorageProvider>
            </ConnectivityProvider>
          </SecurityProvider>
        </PWAProvider>
      </TutorialProvider>
    </ThemeProvider>
  </React.StrictMode>
);
