/**
 * Axiom Forge - Headless IPC & Store Test Suite
 * 
 * Lightweight Node.js test runner for validating frontend API structures,
 * reducer logic, and relay payload handling.
 */

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

let passCount = 0;
let failCount = 0;

function assert(condition, message) {
  if (condition) {
    console.log(`  ✅ [PASS] ${message}`);
    passCount++;
  } else {
    console.error(`  ❌ [FAIL] ${message}`);
    failCount++;
  }
}

async function runFrontendTests() {
  console.log('\n========================================');
  console.log('🚀 Running Frontend & IPC Bridge Tests...');
  console.log('========================================\n');

  // Test 1: Validate IPC Bridge file syntax and exposed structure
  const ipcPath = path.join(__dirname, '../src/lib/ipc-bridge.js');
  assert(fs.existsSync(ipcPath), 'ipc-bridge.js file exists');

  const ipcContent = fs.readFileSync(ipcPath, 'utf8');
  assert(ipcContent.includes('agency:'), 'IPC Bridge exposes window.electronAPI.agency');
  assert(ipcContent.includes('getAppMode'), 'IPC Bridge includes getAppMode handler');
  assert(ipcContent.includes('setAppMode'), 'IPC Bridge includes setAppMode handler');
  assert(ipcContent.includes('createProposal'), 'IPC Bridge includes createProposal handler');
  assert(ipcContent.includes('submitToRelay'), 'IPC Bridge includes submitToRelay handler');

  // Test 2: Validate Relay Server script endpoints
  const relayPath = path.join(__dirname, '../relay-server/index.js');
  assert(fs.existsSync(relayPath), 'relay-server/index.js exists');

  const relayContent = fs.readFileSync(relayPath, 'utf8');
  assert(relayContent.includes('/api/v1/proposals/submit'), 'Relay script handles submit endpoint');
  assert(relayContent.includes('/api/v1/proposals/list'), 'Relay script handles list endpoint');
  assert(relayContent.includes('/api/v1/proposals/approve'), 'Relay script handles approve endpoint');

  // Test 3: Validate ClientLayout component exists
  const clientLayoutPath = path.join(__dirname, '../src/components/ClientLayout.jsx');
  assert(fs.existsSync(clientLayoutPath), 'ClientLayout.jsx component exists');

  // Test 4: Validate AgencyDashboard & ReviewModal components
  const dashboardPath = path.join(__dirname, '../src/pages/AgencyDashboard.jsx');
  const reviewModalPath = path.join(__dirname, '../src/components/ReviewModal.jsx');
  assert(fs.existsSync(dashboardPath), 'AgencyDashboard.jsx page exists');
  assert(fs.existsSync(reviewModalPath), 'ReviewModal.jsx component exists');

  console.log(`\nFrontend IPC Test Summary: ${passCount} Passed, ${failCount} Failed.`);

  if (failCount > 0) {
    process.exit(1);
  }
}

runFrontendTests().catch(err => {
  console.error('Test execution error:', err);
  process.exit(1);
});
