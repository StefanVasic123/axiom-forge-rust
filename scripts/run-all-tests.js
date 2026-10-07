/**
 * Axiom Forge - Master Test Runner & Diagnostic Report Generator
 * 
 * Executes all Rust backend unit/integration tests and Node.js IPC tests.
 * Generates a comprehensive test report artifact (test_report.md) and terminal summary.
 */

import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.join(__dirname, '..');

const startTime = Date.now();

console.log('\n=================================================');
console.log('🧪 Axiom Forge System Diagnostic & Test Runner');
console.log('=================================================\n');

let rustSuccess = false;
let frontendSuccess = false;
let rustOutput = '';
let frontendOutput = '';

// 1. Run Rust Backend Tests
console.log('📦 Executing Rust Backend Unit & Integration Tests (cargo test)...');
try {
  rustOutput = execSync('cargo test', {
    cwd: path.join(projectRoot, 'src-tauri'),
    encoding: 'utf8'
  });
  rustSuccess = true;
  console.log('  ✅ Rust Tests Passed Successfully!\n');
} catch (err) {
  rustOutput = err.stdout + '\n' + err.stderr;
  console.error('  ❌ Rust Tests Failed!\n');
}

// 2. Run Frontend & IPC Tests
console.log('🌐 Executing Frontend IPC & Store Tests...');
try {
  frontendOutput = execSync('node scripts/test-frontend-ipc.js', {
    cwd: projectRoot,
    encoding: 'utf8'
  });
  frontendSuccess = true;
  console.log('  ✅ Frontend IPC Tests Passed Successfully!\n');
} catch (err) {
  frontendOutput = err.stdout + '\n' + err.stderr;
  console.error('  ❌ Frontend IPC Tests Failed!\n');
}

const duration = ((Date.now() - startTime) / 1000).toFixed(2);
const memUsage = Math.round(process.memoryUsage().heapUsed / 1024 / 1024);

// Generate Markdown Report
const reportContent = `# 🧪 Axiom Forge System Diagnostic & Test Report

**Vreme Izvršavanja:** ${new Date().toLocaleString()}  
**Status:** ${rustSuccess && frontendSuccess ? '✅ SVI TESTOVI SU PROŠLI' : '❌ IMATE GREŠKE'}  
**Ukupno Trajanje:** ${duration} sekundi  
**Potrošnja RAM-a:** ${memUsage} MB  

---

## 1. Rezultati Po Modulima

| Modul Testiranja | Status | Napomena |
| :--- | :--- | :--- |
| **Rust Backend Tests (\`cargo test\`)** | ${rustSuccess ? '✅ PASSED' : '❌ FAILED'} | Unit testovi u \`lib.rs\`, \`mode.rs\`, \`git_engine.rs\`, \`relay.rs\` |
| **Frontend IPC & Store Tests** | ${frontendSuccess ? '✅ PASSED' : '❌ FAILED'} | Headless Node.js runner za \`ipc-bridge.js\` i komponente |

---

## 2. Detaljni Logovi Rust Testova

\`\`\`text
${rustOutput.trim()}
\`\`\`

---

## 3. Detaljni Logovi Frontend IPC Testova

\`\`\`text
${frontendOutput.trim()}
\`\`\`
`;

// Save report in project root & artifact dir if available
const localReportPath = path.join(projectRoot, 'test_report.md');
fs.writeFileSync(localReportPath, reportContent);

console.log('=================================================');
console.log(`📊 TEST REPORT GENERATED: ${localReportPath}`);
console.log(`⏱️ Duration: ${duration}s | 💾 RAM: ${memUsage}MB`);
console.log(`Status: ${rustSuccess && frontendSuccess ? '✅ PASSED' : '❌ FAILED'}`);
console.log('=================================================\n');

if (!rustSuccess || !frontendSuccess) {
  process.exit(1);
}
