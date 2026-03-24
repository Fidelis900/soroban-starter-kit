import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { ConnectivityProvider } from './context/ConnectivityContext';
import { StorageProvider } from './context/StorageContext';
import { TransactionQueueProvider } from './context/TransactionQueueContext';
import './styles/index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <ConnectivityProvider>
      <StorageProvider>
        <TransactionQueueProvider>
          <App />
        </TransactionQueueProvider>
      </StorageProvider>
    </ConnectivityProvider>
  </React.StrictMode>
);
