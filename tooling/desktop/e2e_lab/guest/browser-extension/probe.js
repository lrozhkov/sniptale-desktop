const endpoint = 'http://127.0.0.1:18765/result';
const nonce = '__SNIPTALE_E2E_NONCE__';

async function report(browser, status, detail) {
  document.title = `sniptale-e2e-result-${nonce}-${browser}-${status}`;
  try {
    await fetch(endpoint, {
      method: 'POST',
      headers: {'content-type': 'text/plain;charset=UTF-8'},
      body: JSON.stringify({browser, status, detail, nonce}),
      keepalive: true,
    });
  } catch {
    // The bounded guest runner owns timeout/failure reporting when loopback is unavailable.
  }
}

function probe() {
  const browser = navigator.userAgent.includes('Edg/') ? 'edge' : 'chrome';
  const port = chrome.runtime.connectNative('com.sniptale.dev.native_host');
  let completed = false;
  let staleCommandSent = false;
  const timeout = setTimeout(() => {
    if (!completed) report(browser, 'fail', 'native response timeout').catch(() => { document.title = 'sniptale-e2e-report-failed'; });
  }, 45000);
  port.onMessage.addListener((message) => {
    if (message && message.type === 'app.hello') {
      if (message.protocolVersion !== 2 || message.settingsAuthority !== 'desktop' || !/^[0-9a-f]{64}$/.test(message.appIdentity?.clientId ?? '')) {
        completed = true; clearTimeout(timeout);
        report(browser, 'fail', 'Desktop identity or authority mismatch').finally(() => port.disconnect()).catch(() => { document.title = 'sniptale-e2e-report-failed'; });
        return;
      }
      port.postMessage({
        type: 'extension.controller.acquire',
        protocolVersion: 2,
        connectionId: `e2e-${browser}-connection`,
        browserFamily: browser,
        profileKey: 'e2e-profile',
        documentId: `e2e-${browser}-document`,
        requestedAtEpochMs: Date.now(),
        reason: 'initial-connect',
      });
    } else if (message && message.type === 'app.controller.lease' && ['selected', 'available', 'selection-required'].includes(message.selectionStatus) && !staleCommandSent) {
      staleCommandSent = true;
      port.postMessage({
        type: 'extension.screenshot.chunkRequest',
        protocolVersion: 2,
        controllerLeaseId: 'lease:stale-e2e',
        captureId: 'capture-e2e',
        chunkIndex: 0,
      });
    } else if (message && message.type === 'app.operation.failed') {
      const code = message.error?.code || message.code || 'missing-error-code';
      completed = true;
      clearTimeout(timeout);
      const status = staleCommandSent && code === 'stale-controller-lease' ? 'pass' : 'fail';
      report(browser, status, `app.hello, controller lease, and operation failure received (${code})`)
        .finally(() => port.disconnect()).catch(() => { document.title = 'sniptale-e2e-report-failed'; });
    }
  });
  port.onDisconnect.addListener(() => {
    if (!completed) {
      completed = true;
      clearTimeout(timeout);
      report(browser, 'fail', chrome.runtime.lastError?.message || 'native port disconnected')
        .catch(() => { document.title = 'sniptale-e2e-report-failed'; });
    }
  });
  port.postMessage({
    type: 'extension.hello',
    protocolVersion: 2,
    settingsSchemaVersion: 1,
    connectionIntent: 'user-requested',
    capabilities: ['desktop-settings-v1', 'durable-media-ack-v1'],
    extensionVersion: '0.1.0',
    extensionId: chrome.runtime.id,
    minAppVersion: '0.1.0',
    supportedProtocolVersions: [2],
    supportedSettingsSchemaVersions: [1],
  });
}

probe();
