const CORS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type',
}

export default {
  async fetch(request, env, ctx) {
    const url = new URL(request.url)
    const path = url.pathname

    if (request.method === 'OPTIONS') {
      return new Response(null, { headers: CORS })
    }

    // Serve frontend from GitHub Pages
    const htmlResp = await fetch('https://projecthorizon993.github.io/bus-tracker-rust/', {
      headers: { 'Accept': 'text/html' },
    })
    const html = await htmlResp.text()

    return new Response(html, {
      status: htmlResp.status,
      headers: { ...CORS, 'Content-Type': 'text/html; charset=utf-8' },
    })
  },
}
