import { expect, Page } from '@playwright/test';
import { ChildProcess, spawn } from 'child_process';
import * as fs from 'fs';
import * as http from 'http';
import * as net from 'net';
import * as os from 'os';
import * as path from 'path';

const PNG_1X1 = Buffer.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0x63,
  0x34, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
  0x42, 0x60, 0x82,
]);

function getFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.unref();
    srv.on('error', reject);
    srv.listen(0, '127.0.0.1', () => {
      const addr = srv.address();
      if (!addr || typeof addr === 'string') {
        srv.close(() => reject(new Error('Failed to get free port')));
        return;
      }
      const port = addr.port;
      srv.close((err) => (err ? reject(err) : resolve(port)));
    });
  });
}

export interface TestServerOptions {
  basePath?: string;
  demoMode?: boolean;
  sfwMode?: boolean;
  includeAdultModule?: boolean;
  databaseUrl?: string;
  env?: Record<string, string>;
}

export interface TestServerHandle {
  baseUrl: string;
  originUrl: string;
  internalToken: string;
  stop: () => Promise<void>;
}

export function attachPageErrorGuard(page: Page): () => void {
  const errors: string[] = [];
  page.on('pageerror', (err) => {
    errors.push(`pageerror: ${err.message}`);
  });
  return () => {
    expect(errors, `Uncaught browser page errors: ${errors.join('; ')}`).toEqual([]);
  };
}

export async function startTestServer(opts: TestServerOptions = {}): Promise<TestServerHandle> {
  const repoRoot = path.resolve(__dirname, '../..');
  const fmdBin = process.env.FMD_BIN || path.join(repoRoot, 'target', 'debug', 'fmd');
  if (!fs.existsSync(fmdBin)) {
    throw new Error(`fmd binary not found at ${fmdBin}. Run 'cargo build --bin fmd' first.`);
  }

  // 1. Start mock manga origin server
  let originUrl = '';
  const originServer = http.createServer((req, res) => {
    const reqUrl = req.url || '/';
    if (reqUrl === '/catalog/0') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(
        '<a class="item" href="/manga/one">One Manga</a>' +
          '<a class="item" href="/manga/two">Two Manga</a>',
      );
      return;
    }
    if (reqUrl === '/manga/one') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`
        <html><body>
          <h1 class="title">One Manga</h1>
          <span class="author">Author E2E</span>
          <span class="status">Ongoing</span>
          <span class="genre">Action</span>
          <span class="genre">Adventure</span>
          <p class="summary">E2E Summary</p>
          <img class="cover" src="${originUrl}/cover.png" />
          <div class="chapters">
            <a href="/ch/1">Chapitre 1</a>
            <a href="/ch/2">Chapitre 2</a>
          </div>
        </body></html>
      `);
      return;
    }
    if (reqUrl === '/manga/adult-one') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`
        <html><body>
          <h1 class="title">Adult One</h1>
          <span class="author">Author X</span>
          <span class="status">Ongoing</span>
          <span class="genre">Action</span>
          <span class="genre">Ecchi</span>
          <p class="summary">Adult Summary</p>
          <div class="chapters"><a href="/ch/1">Ch 1</a></div>
        </body></html>
      `);
      return;
    }
    if (reqUrl === '/ch/1' || reqUrl === '/ch/2') {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`<img class="page" src="${originUrl}/img/1.png" />`);
      return;
    }
    if (reqUrl === '/cover.png' || reqUrl === '/img/1.png') {
      res.writeHead(200, { 'Content-Type': 'image/png' });
      res.end(PNG_1X1);
      return;
    }
    res.writeHead(404);
    res.end('Not found');
  });

  await new Promise<void>((resolve) => {
    originServer.listen(0, '127.0.0.1', () => {
      const addr = originServer.address() as net.AddressInfo;
      originUrl = `http://127.0.0.1:${addr.port}`;
      resolve();
    });
  });

  // 2. Prepare isolated working dir with Lua fixture modules
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'fmd3-e2e-'));
  const modulesDir = path.join(tmpDir, 'lua', 'modules');
  fs.mkdirSync(modulesDir, { recursive: true });
  fs.mkdirSync(path.join(tmpDir, 'data'), { recursive: true });

  fs.writeFileSync(
    path.join(modulesDir, 'FixtureManga.lua'),
    `
function Init()
    local m = NewWebsiteModule()
    m.ID = 'fixture-mod'
    m.Name = 'FixtureManga'
    m.RootURL = '${originUrl}'
    m.Category = 'English'
    m.TotalDirectory = 1
    m.OnGetNameAndLink = 'GetNameAndLink'
    m.OnGetInfo = 'GetInfo'
    m.OnGetPageNumber = 'GetPageNumber'
end
function GetNameAndLink()
    if HTTP.GET(MODULE.RootURL .. '/catalog/' .. URL) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        x.XPathHREFAll('//a[@class="item"]', LINKS, NAMES)
        return no_error
    end
    return net_problem
end
function GetInfo()
    if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        MANGAINFO.Title = x.XPathString('//h1[@class="title"]')
        MANGAINFO.Authors = x.XPathString('//span[@class="author"]')
        MANGAINFO.Status = MangaInfoStatusIfPos(x.XPathString('//span[@class="status"]'))
        MANGAINFO.Summary = x.XPathString('//p[@class="summary"]')
        MANGAINFO.CoverLink = x.XPathString('//img[@class="cover"]/@src')
        MANGAINFO.Genres = x.XPathStringAll('//span[@class="genre"]')
        x.XPathHREFAll('//div[@class="chapters"]/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
        return no_error
    end
    return net_problem
end
function GetPageNumber()
    if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        x.XPathStringAll('//img[@class="page"]/@src', TASK.PageLinks)
        return no_error
    end
    return net_problem
end
`,
  );

  if (opts.includeAdultModule) {
    fs.writeFileSync(
      path.join(modulesDir, 'NHentaiFixture.lua'),
      `
function Init()
    local m = NewWebsiteModule()
    m.ID = 'nhentai-fixture'
    m.Name = 'NHentai'
    m.RootURL = '${originUrl}'
    m.Category = 'H-Sites'
end
`,
    );
  }

  // 3. Spawn fmd serve
  const port = await getFreePort();
  const basePath = opts.basePath || '';
  const baseUrl = `http://127.0.0.1:${port}`;
  const internalToken = 'e2e-internal-token';

  const childEnv: NodeJS.ProcessEnv = {
    ...process.env,
    RUST_LOG: 'warn',
    BASE_PATH: basePath,
    FMD_INTERNAL_TOKEN: internalToken,
    FMD_DEMO_MODE: opts.demoMode ? 'true' : 'false',
    FMD_SFW_MODE: opts.sfwMode ? 'true' : 'false',
    ...opts.env,
  };
  delete childEnv.FMD_NATS_URL;
  if (opts.databaseUrl) {
    childEnv.FMD_DATABASE_URL = opts.databaseUrl;
  } else {
    delete childEnv.FMD_DATABASE_URL;
  }
  if (!opts.env?.FMD_ADMIN_USER) {
    delete childEnv.FMD_ADMIN_USER;
  }
  if (!opts.env?.FMD_ADMIN_PASSWORD) {
    delete childEnv.FMD_ADMIN_PASSWORD;
  }
  if (!opts.env?.FMD_ADMIN_PASSWORD_FILE) {
    delete childEnv.FMD_ADMIN_PASSWORD_FILE;
  }

  let logs = '';
  const proc: ChildProcess = spawn(fmdBin, ['serve', '--bind', `127.0.0.1:${port}`], {
    cwd: tmpDir,
    env: childEnv,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  proc.stdout?.on('data', (d) => (logs += d.toString()));
  proc.stderr?.on('data', (d) => (logs += d.toString()));

  // Poll /api/health
  const healthUrl = `${baseUrl}${basePath}/api/health`;
  const deadline = Date.now() + 15_000;
  let ready = false;
  while (Date.now() < deadline) {
    if (proc.exitCode !== null) {
      throw new Error(`fmd serve exited early with code ${proc.exitCode}:\n${logs}`);
    }
    try {
      const res = await fetch(healthUrl);
      if (res.ok) {
        ready = true;
        break;
      }
    } catch {
      // retry
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  if (!ready) {
    proc.kill('SIGKILL');
    throw new Error(`fmd serve did not become healthy at ${healthUrl}:\n${logs}`);
  }

  return {
    baseUrl,
    originUrl,
    internalToken,
    stop: async () => {
      if (proc.exitCode === null) {
        proc.kill('SIGTERM');
        await new Promise<void>((resolve) => {
          const timer = setTimeout(() => {
            proc.kill('SIGKILL');
            resolve();
          }, 3000);
          proc.once('exit', () => {
            clearTimeout(timer);
            resolve();
          });
        });
      }
      await new Promise<void>((resolve) => originServer.close(() => resolve()));
      fs.rmSync(tmpDir, { recursive: true, force: true });
    },
  };
}
