/**
 * Axiom Agency Relay Server (Cloudflare Worker / Serverless Node.js)
 * 
 * Highly efficient, zero-cost ($0/mo) relay bridge for connecting client Axiom apps
 * with developer review dashboards via Webhooks and Telegram/Email notifications.
 */

// In-memory or KV proposal store (for Cloudflare Workers KV or memory fallback)
const proposalsStore = new Map();

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname;

    // CORS Headers
    const corsHeaders = {
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type, Authorization, X-Agency-Key',
    };

    if (request.method === 'OPTIONS') {
      return new Response(null, { headers: corsHeaders });
    }

    try {
      // 1. Submit Proposal (Called by Client Axiom App)
      if (path === '/api/v1/proposals/submit' && request.method === 'POST') {
        const body = await request.json();
        const { setupKey, projectId, prompt, branchName, files } = body;

        if (!setupKey || !projectId) {
          return new Response(JSON.stringify({ error: 'Missing setupKey or projectId' }), {
            status: 400,
            headers: { ...corsHeaders, 'Content-Type': 'application/json' },
          });
        }

        const proposalId = `proposal_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
        const proposal = {
          id: proposalId,
          setupKey,
          projectId,
          prompt,
          branchName: branchName || `client-proposal/${Date.now()}`,
          files: files || [],
          status: 'pending_review',
          createdAt: new Date().toISOString(),
        };

        // Store proposal
        proposalsStore.set(proposalId, proposal);

        // Optional: Send Telegram Notification if bot token is provided
        if (env?.TELEGRAM_BOT_TOKEN && env?.TELEGRAM_CHAT_ID) {
          const text = `🚨 *Novi Klijentski Predlog u Axiom-u!*\n\n*Projekat:* \`${projectId}\`\n*Prompt:* "${prompt}"\n*Grana:* \`${proposal.branchName}\`\n\n[Otvori Dashboard u Axiom Forge]`;
          await fetch(`https://api.telegram.org/bot${env.TELEGRAM_BOT_TOKEN}/sendMessage`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              chat_id: env.TELEGRAM_CHAT_ID,
              text,
              parse_mode: 'Markdown',
            }),
          });
        }

        return new Response(JSON.stringify({ success: true, proposal }), {
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      // 2. List Proposals (Called by Developer Review Dashboard)
      if (path === '/api/v1/proposals/list' && request.method === 'GET') {
        const apiKey = request.headers.get('X-Agency-Key') || url.searchParams.get('key');
        const proposalsList = Array.from(proposalsStore.values()).filter(p => !apiKey || p.setupKey === apiKey);

        return new Response(JSON.stringify({ success: true, proposals: proposalsList }), {
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      // 3. Approve Proposal Trigger (Called on 1-Click Approve)
      if (path === '/api/v1/proposals/approve' && request.method === 'POST') {
        const body = await request.json();
        const { proposalId } = body;

        if (proposalsStore.has(proposalId)) {
          const proposal = proposalsStore.get(proposalId);
          proposal.status = 'approved';
          proposalsStore.set(proposalId, proposal);
        }

        return new Response(JSON.stringify({ success: true, message: 'Proposal approved on relay' }), {
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      // Health Check
      if (path === '/' || path === '/health') {
        return new Response(JSON.stringify({ status: 'ok', service: 'Axiom Agency Relay Bridge v1.0' }), {
          headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        });
      }

      return new Response(JSON.stringify({ error: 'Endpoint not found' }), {
        status: 404,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    } catch (err) {
      return new Response(JSON.stringify({ error: err.message }), {
        status: 500,
        headers: { ...corsHeaders, 'Content-Type': 'application/json' },
      });
    }
  },
};
