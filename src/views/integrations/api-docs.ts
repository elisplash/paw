// src/views/integrations/api-docs.ts
//
// API documentation URLs and key-generation paths for services that use
// generic HTTP Request + API Key auth.  Keeps the catalog data-only while
// giving users a clickable link and specific instructions.
//
// Format:  serviceId → [apiDocsUrl, keyPath]
//   keyPath = short human phrase like "Settings → API Keys"

const D: Record<string, [url: string, keyPath: string]> = {
  // ── Communication ──
  crisp: ['https://docs.crisp.chat/references/rest-api/', 'Settings → API Keys'],
  'tawk-to': ['https://developer.tawk.to/', 'Administration → API Access'],
  livechat: ['https://developers.livechat.com/', 'Settings → API & Webhooks'],
  'front-app': ['https://dev.frontapp.com/', 'Settings → API & Integrations → API tokens'],
  ringcentral: ['https://developers.ringcentral.com/', 'Developer Portal → Apps → Credentials'],
  viber: ['https://developers.viber.com/', 'Admin Panel → Create Bot → Token'],
  'signal-api': ['https://github.com/bbernhard/signal-cli-rest-api', 'Self-hosted API config'],
  freshchat: ['https://developers.freshchat.com/', 'Settings → API Tokens'],
  workplace: ['https://developers.facebook.com/docs/workplace/', 'Admin Panel → Integrations'],
  genesys: ['https://developer.genesys.cloud/', 'Admin → Integrations → OAuth'],
  kayako: ['https://developer.kayako.com/', 'Admin → API → API Keys'],
  groove: ['https://www.groovehq.com/docs/api', 'Settings → API → Access Token'],
  'discord-webhook': [
    'https://discord.com/developers/docs/resources/webhook',
    'Server Settings → Integrations → Webhooks',
  ],
  'slack-webhook': [
    'https://api.slack.com/messaging/webhooks',
    'api.slack.com → Incoming Webhooks',
  ],

  // ── Development & DevOps ──
  datadog: ['https://docs.datadoghq.com/api/', 'Organization Settings → API Keys'],
  vercel: ['https://vercel.com/docs/rest-api', 'Settings → Tokens → Create'],
  'github-actions': [
    'https://docs.github.com/en/rest/actions',
    'Settings → Developer settings → Personal access tokens',
  ],
  drone: ['https://docs.drone.io/api/', 'Account Settings → Token'],
  buildkite: ['https://buildkite.com/docs/apis/rest-api', 'Personal Settings → API Access Tokens'],
  teamcity: [
    'https://www.jetbrains.com/help/teamcity/rest-api.html',
    'My Settings → Access Tokens',
  ],
  sonarqube: [
    'https://docs.sonarqube.org/latest/extension-guide/web-api/',
    'My Account → Security → Tokens',
  ],
  'new-relic': ['https://docs.newrelic.com/docs/apis/', 'API Keys page → Create a key'],
  prometheus: [
    'https://prometheus.io/docs/prometheus/latest/querying/api/',
    'Self-hosted — no key needed for basic setup',
  ],
  statuspage: ['https://developer.statuspage.io/', 'Manage Account → API → API Key'],
  opsgenie: ['https://docs.opsgenie.com/docs/api-overview', 'Settings → API key management'],
  heroku: [
    'https://devcenter.heroku.com/articles/platform-api-reference',
    'Account Settings → API Key',
  ],
  railway: ['https://docs.railway.com/reference/public-api', 'Account Settings → Tokens → Create'],
  render: ['https://docs.render.com/api', 'Account Settings → API Keys'],
  'fly-io': ['https://fly.io/docs/machines/api/', 'Dashboard → Account → Access Tokens'],
  'docker-hub': [
    'https://docs.docker.com/docker-hub/api/latest/',
    'Account Settings → Security → Access Tokens',
  ],
  'terraform-cloud': [
    'https://developer.hashicorp.com/terraform/cloud-docs/api-docs',
    'User Settings → Tokens → Create an API token',
  ],
  'snyk-dev': ['https://docs.snyk.io/snyk-api', 'Account Settings → General → Auth Token'],
  gitea: ['https://gitea.io/en-us/docs/api/', 'Settings → Applications → Manage Access Tokens'],
  harbor: ['https://goharbor.io/docs/', 'User Profile → CLI Secret'],
  argocd: [
    'https://argo-cd.readthedocs.io/en/stable/developer-guide/api-docs/',
    'Settings → Accounts → Generate token',
  ],
  launchdarkly: [
    'https://apidocs.launchdarkly.com/',
    'Account Settings → Authorization → Access tokens',
  ],
  codecov: ['https://docs.codecov.com/reference/overview', 'Settings → Global Upload Token'],
  rollbar: ['https://docs.rollbar.com/reference', 'Settings → Project Access Tokens'],
  honeybadger: ['https://docs.honeybadger.io/api/', 'Settings → Authentication → API token'],
  dependabot: [
    'https://docs.github.com/en/rest/dependabot',
    'GitHub → Settings → Developer settings → Tokens',
  ],
  semaphore: ['https://docs.semaphoreci.com/reference/api/', 'Account → Settings → API'],
  codefresh: [
    'https://codefresh.io/docs/docs/integrations/codefresh-api/',
    'User Settings → API Keys → Generate',
  ],
  pulumi: [
    'https://www.pulumi.com/docs/pulumi-cloud/reference/api/',
    'Settings → Access Tokens → Create token',
  ],
  'source-graph': ['https://docs.sourcegraph.com/api/', 'Settings → Access tokens → Generate'],
  raygun: [
    'https://raygun.com/documentation/product-guides/integrations/',
    'Application Settings → API Key',
  ],

  // ── Productivity ──
  basecamp: ['https://github.com/basecamp/bc3-api', 'Launchpad → Integrations → API'],
  wrike: ['https://developers.wrike.com/', 'Apps & Integrations → API → Permanent token'],
  smartsheet: [
    'https://smartsheet.redoc.ly/',
    'Account → Personal Settings → API Access → Generate token',
  ],
  teamwork: ['https://developer.teamwork.com/', 'Profile → Edit My Profile → API & Mobile'],
  ticktick: ['https://developer.ticktick.com/', 'Settings → Integrations → API'],
  evernote: ['https://dev.evernote.com/doc/', 'Developer portal → API Keys'],
  confluence: [
    'https://developer.atlassian.com/cloud/confluence/rest/',
    'Profile → Manage Account → Security → API tokens',
  ],
  miro: ['https://developers.miro.com/reference', 'Profile → Settings → API → Token'],
  'google-forms': [
    'https://developers.google.com/forms/api',
    'Google Cloud Console → Credentials → API Key',
  ],
  'microsoft-planner': [
    'https://learn.microsoft.com/en-us/graph/api/resources/planner-overview',
    'Azure Portal → App Registrations',
  ],
  lucidchart: ['https://developer.lucid.co/', 'Account → API → Tokens'],
  whimsical: ['https://whimsical.com/', 'Account → API access'],

  // ── HR & Recruiting ──
  gusto: ['https://docs.gusto.com/app-integrations/reference', 'Developer Portal → API tokens'],
  lever: [
    'https://hire.lever.co/developer/documentation',
    'Settings → Integrations → API Credentials',
  ],
  greenhouse: [
    'https://developers.greenhouse.io/',
    'Configure → Dev Center → API Credential Management',
  ],
  deel: ['https://developer.deel.com/', 'Organization Settings → Apps & Integrations → API'],
  rippling: ['https://developer.rippling.com/', 'Admin → Company Settings → API Access'],
  lattice: ['https://developers.lattice.com/', 'Admin → Integrations → API'],

  // ── CRM ──
  freshsales: ['https://developers.freshworks.com/crm/', 'Settings → API Settings → API Key'],
  insightly: ['https://api.insight.ly/v3.1/Help', 'User Settings → API Key'],
  sugarcrm: ['https://support.sugarcrm.com/Documentation/Sugar_Developer/', 'Admin → OAuth Keys'],
  'close-crm': ['https://developer.close.com/', 'Settings → API Keys → Generate'],
  nutshell: ['https://developers.nutshell.com/', 'Setup → API Keys'],
  streak: ['https://www.streak.com/api', 'Settings → Integrations → Streak API → API Key'],
  bitrix24: ['https://training.bitrix24.com/rest_help/', 'Applications → Developer resources'],
  'capsule-crm': [
    'https://developer.capsulecrm.com/',
    'My Preferences → API Authentication Tokens',
  ],
  nimble: ['https://nimble.readthedocs.io/', 'Settings → API Tokens'],
  apollo: [
    'https://apolloio.github.io/apollo-api-docs/',
    'Settings → Integrations → API → API Key',
  ],
  outreach: ['https://api.outreach.io/api/v2/docs', 'Settings → API & Integrations'],
  salesloft: ['https://developers.salesloft.com/', 'Settings → Integrations → API → Generate Key'],

  // ── Commerce & Payments ──
  square: [
    'https://developer.squareup.com/reference/square',
    'Developer Dashboard → Applications → Credentials',
  ],
  bigcommerce: [
    'https://developer.bigcommerce.com/docs/rest',
    'Settings → API → API Accounts → Create API Account',
  ],
  'lemon-squeezy': ['https://docs.lemonsqueezy.com/api', 'Settings → API → Create API Key'],
  recurly: ['https://developers.recurly.com/', 'Integrations → API Credentials → API Key'],
  braintree: [
    'https://developer.paypal.com/braintree/docs/reference/overview',
    'Settings → API → API Keys → Generate',
  ],
  mollie: ['https://docs.mollie.com/', 'Dashboard → Developers → API keys'],
  razorpay: [
    'https://razorpay.com/docs/api/',
    'Dashboard → Account & Settings → API Keys → Generate Key',
  ],
  'coinbase-commerce': [
    'https://docs.cdp.coinbase.com/',
    'Settings → API keys → Create an API key',
  ],
  freshbooks: ['https://www.freshbooks.com/api', 'Developer Portal → Create an App'],
  wave: ['https://developer.waveapps.com/', 'Developer Portal → Applications → Create'],
  ecwid: ['https://api-docs.ecwid.com/', 'Control Panel → Apps → API'],
  snipcart: ['https://docs.snipcart.com/v3/api-reference/', 'Account → API Keys'],
  plaid: ['https://plaid.com/docs/api/', 'Dashboard → Team Settings → Keys'],
  adyen: ['https://docs.adyen.com/api-explorer/', 'Customer Area → Developers → API credentials'],
  fastspring: ['https://developer.fastspring.com/', 'Integrations → API Credentials'],
  printful: ['https://developers.printful.com/', 'Settings → API → Create API Key'],

  // ── Social Media ──
  pinterest: [
    'https://developers.pinterest.com/',
    'Pinterest Business → Apps → Create → Access Token',
  ],
  mastodon: ['https://docs.joinmastodon.org/api/', 'Preferences → Development → New Application'],
  tiktok: ['https://developers.tiktok.com/', 'Developer Portal → Manage apps → API Key'],
  bluesky: ['https://docs.bsky.app/', 'Settings → App Passwords → Add App Password'],
  threads: ['https://developers.facebook.com/docs/threads/', 'Meta Developer Portal → Apps'],
  buffer: ['https://bufferapp.com/developers/api', 'Settings → Manage Apps → Access Token'],
  hootsuite: ['https://developer.hootsuite.com/', 'Developer Portal → My Apps → Credentials'],
  later: ['https://developers.later.com/', 'Settings → Integrations → API'],
  tumblr: [
    'https://www.tumblr.com/docs/en/api/v2',
    'Settings → Apps → OAuth Applications → Register',
  ],
  snapchat: ['https://marketingapi.snapchat.com/docs/', 'Business Manager → Developer → OAuth App'],
  'twitch-social': ['https://dev.twitch.tv/docs/api/', 'Developer Console → Applications → Manage'],
  producthunt: ['https://api.producthunt.com/v2/docs', 'Settings → API Dashboard → Create Token'],
  devto: [
    'https://developers.forem.com/api',
    'Settings → Extensions → DEV Community API Keys → Generate',
  ],
  instagram: [
    'https://developers.facebook.com/docs/instagram-api/',
    'Meta Developer Portal → Apps → Instagram',
  ],
  vk: ['https://dev.vk.com/reference', 'My Apps → Settings → Access Token'],
  weibo: ['https://open.weibo.com/wiki/API', 'Open Platform → My Apps → App Key'],

  // ── Cloud & Infrastructure ──
  digitalocean: [
    'https://docs.digitalocean.com/reference/api/',
    'API → Tokens → Generate New Token',
  ],
  gcp: [
    'https://cloud.google.com/docs/authentication/api-keys',
    'Cloud Console → APIs & Services → Credentials → Create',
  ],
  azure: [
    'https://learn.microsoft.com/en-us/rest/api/azure/',
    'Azure Portal → App Registrations → Certificates & secrets',
  ],
  linode: ['https://www.linode.com/docs/api/', 'Profile → API Tokens → Create a Token'],
  vultr: ['https://www.vultr.com/api/', 'Account → API → Enable API → Copy API Key'],
  hetzner: ['https://docs.hetzner.cloud/', 'Project → Security → API Tokens → Generate'],
  backblaze: [
    'https://www.backblaze.com/docs/cloud-storage',
    'Account → Application Keys → Add a New Key',
  ],
  wasabi: ['https://docs.wasabi.com/', 'Access Keys → Create New Access Key'],
  'aws-ec2': [
    'https://docs.aws.amazon.com/AWSEC2/latest/APIReference/',
    'IAM Console → Users → Security credentials → Access keys',
  ],
  'aws-cloudwatch': [
    'https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/',
    'IAM Console → Users → Security credentials → Access keys',
  ],
  'azure-functions': [
    'https://learn.microsoft.com/en-us/azure/azure-functions/functions-reference',
    'Azure Portal → Function App → App Keys',
  ],
  gcs: [
    'https://cloud.google.com/storage/docs/json_api',
    'Cloud Console → APIs & Services → Credentials',
  ],
  kubernetes: ['https://kubernetes.io/docs/reference/', 'Cluster → Service Account → Token'],
  'oracle-cloud': ['https://docs.oracle.com/en-us/iaas/api/', 'Profile → API Keys → Add API Key'],
  'ibm-cloud': ['https://cloud.ibm.com/apidocs', 'Manage → Access → API keys → Create'],
  tailscale: ['https://tailscale.com/api', 'Settings → Keys → Generate auth key'],
  zerotier: ['https://docs.zerotier.com/central/v1', 'Account → Create API Access Token'],
  portainer: [
    'https://docs.portainer.io/api/docs',
    'Settings → Account → API Key → Add Access Token',
  ],

  // ── Storage ──
  minio: [
    'https://min.io/docs/minio/linux/developers/minio-drivers.html',
    'Console → Access Keys → Create access key',
  ],
  pcloud: ['https://docs.pcloud.com/', 'Developer → My Applications → API Token'],
  mega: ['https://mega.nz/sdk', 'Account → API → Credential'],
  'sharepoint-files': [
    'https://learn.microsoft.com/en-us/sharepoint/dev/',
    'Azure Portal → App Registrations',
  ],
  's3-compatible': [
    'https://docs.aws.amazon.com/AmazonS3/latest/API/',
    'Provider dashboard → Access Keys',
  ],
  webdav: ['https://www.ietf.org/rfc/rfc4918.txt', 'Server settings → Users → Credentials'],
  syncthing: ['https://docs.syncthing.net/rest/', 'Actions → Settings → GUI → API Key'],

  // ── Databases ──
  mariadb: ['https://mariadb.com/kb/en/mariadb-server/', 'Database server → User credentials'],
  cockroachdb: ['https://www.cockroachlabs.com/docs/', 'Console → Cluster → SQL Users'],
  cassandra: ['https://cassandra.apache.org/doc/', 'Cluster configuration → Authentication'],
  neo4j: ['https://neo4j.com/docs/http-api/', 'Database → DBMS → Connect'],
  influxdb: ['https://docs.influxdata.com/influxdb/api/', 'Load Data → API Tokens → Generate'],
  redshift: [
    'https://docs.aws.amazon.com/redshift/latest/APIReference/',
    'IAM Console → Users → Security credentials',
  ],
  clickhouse: [
    'https://clickhouse.com/docs/en/interfaces/http',
    'Database settings → Users → Access tokens',
  ],
  arangodb: ['https://www.arangodb.com/docs/stable/http/', 'Database → Users → Permissions'],
  couchdb: ['https://docs.couchdb.org/en/stable/api/', 'Fauxton → Config → Admin credentials'],
  sqlite: ['https://www.sqlite.org/docs.html', 'Local file — no API key needed'],
  fauna: [
    'https://docs.fauna.com/fauna/current/reference/http/',
    'Dashboard → Security → Keys → Create Key',
  ],
  planetscale: [
    'https://api-docs.planetscale.com/',
    'Organization → Settings → Service tokens → Create',
  ],
  neon: ['https://api-docs.neon.tech/', 'Account → API keys → Create API key'],
  turso: ['https://docs.turso.tech/reference', 'Dashboard → Settings → Platform API Tokens'],
  upstash: [
    'https://upstash.com/docs/redis/rest/rest-api',
    'Console → Account → Management API → Create',
  ],
  surrealdb: ['https://surrealdb.com/docs/integration/http', 'Database → Users → Credentials'],

  // ── Analytics ──
  mixpanel: [
    'https://developer.mixpanel.com/reference/overview',
    'Settings → Organization Settings → Service Accounts',
  ],
  amplitude: [
    'https://www.docs.developers.amplitude.com/analytics/apis/',
    'Settings → Organization Settings → API Keys',
  ],
  heap: ['https://developers.heap.io/reference', 'App → Account → API Keys'],
  plausible: ['https://plausible.io/docs/stats-api', 'Settings → API Keys → Create API Key'],
  fathom: ['https://usefathom.com/api', 'Settings → API → Create Token'],
  matomo: [
    'https://developer.matomo.org/api-reference/',
    'Settings → Personal → Security → Auth tokens → Create',
  ],
  hotjar: ['https://developer.hotjar.com/', 'Organization → API key → Create'],
  fullstory: ['https://developer.fullstory.com/', 'Settings → Integrations → API Key'],
  logrocket: ['https://docs.logrocket.com/reference', 'Settings → General → API Key'],
  clarity: ['https://learn.microsoft.com/en-us/clarity/', 'Settings → API Access → Key'],
  semrush: ['https://developer.semrush.com/', 'Subscription Info → API units → API Key'],
  ahrefs: ['https://docs.ahrefs.com/reference/', 'Account → API → API Key'],
  chartmogul: ['https://dev.chartmogul.com/', 'Admin → API Keys'],
  baremetrics: ['https://developers.baremetrics.com/', 'Settings → API → API Key'],

  // ── Security & Identity ──
  auth0: ['https://auth0.com/docs/api', 'Applications → APIs → API Explorer → Token'],
  vault: ['https://developer.hashicorp.com/vault/api-docs', 'Vault CLI or UI → Token'],
  onelogin: ['https://developers.onelogin.com/', 'Administration → Developers → API Credentials'],
  duo: ['https://duo.com/docs/adminapi', 'Admin Panel → Applications → Admin API → Keys'],
  onepassword: [
    'https://developer.1password.com/docs/connect/',
    'Settings → Integrations → Directory → Access Token',
  ],
  crowdstrike: [
    'https://falcon.crowdstrike.com/documentation/',
    'Support → API Clients and Keys → Create',
  ],
  sentinelone: [
    'https://usea1.sentinelone.net/api-doc/',
    'Settings → Users → API Token → Generate',
  ],
  qualys: ['https://www.qualys.com/docs/qualys-api/', 'Administration → Users → API credentials'],
  tenable: [
    'https://developer.tenable.com/reference/',
    'Settings → My Account → API Keys → Generate',
  ],
  wiz: ['https://docs.wiz.io/wiz-docs', 'Settings → Service Accounts → Create → API Token'],
  sophos: ['https://developer.sophos.com/', 'Sophos Central → API Credentials → Add credential'],
  keycloak: ['https://www.keycloak.org/docs-api', 'Admin Console → Realm Settings → Credentials'],
  jumpcloud: ['https://docs.jumpcloud.com/api/', 'Admin Portal → API Settings → API Key'],

  // ── AI & ML ──
  huggingface: [
    'https://huggingface.co/docs/api-inference/',
    'Settings → Access Tokens → New token',
  ],
  pinecone: ['https://docs.pinecone.io/reference', 'Console → API Keys → Create API Key'],
  anthropic: ['https://docs.anthropic.com/en/docs/', 'Console → API Keys → Create Key'],
  'google-ai': ['https://ai.google.dev/docs', 'Google AI Studio → Get API key'],
  cohere: ['https://docs.cohere.com/reference/', 'Dashboard → API Keys'],
  replicate: [
    'https://replicate.com/docs/reference/http',
    'Account Settings → API tokens → Create token',
  ],
  'stability-ai': [
    'https://platform.stability.ai/docs/api-reference',
    'Account → API Keys → Create',
  ],
  elevenlabs: ['https://elevenlabs.io/docs/api-reference', 'Profile → API Key'],
  assemblyai: ['https://www.assemblyai.com/docs/', 'Account → API Key'],
  deepgram: ['https://developers.deepgram.com/', 'Console → API Keys → Create a Key'],
  weaviate: ['https://weaviate.io/developers/weaviate/api/rest', 'WCD Console → API Keys'],
  qdrant: ['https://qdrant.tech/documentation/', 'Cloud Console → Cluster → API Keys'],
  chromadb: ['https://docs.trychroma.com/reference/', 'Self-hosted — configure auth in settings'],
  groq: ['https://console.groq.com/docs/', 'Console → API Keys → Create API Key'],
  'together-ai': ['https://docs.together.ai/reference/', 'Settings → API Keys → Create'],
  'fireworks-ai': ['https://docs.fireworks.ai/', 'Account → API Keys → Create Key'],
  wolfram: ['https://products.wolframalpha.com/api', 'Developer Portal → My Apps → Get AppID'],
  midjourney: ['https://docs.midjourney.com/', 'Account → Subscription → API'],
  claude: ['https://docs.anthropic.com/en/docs/', 'Console → API Keys → Create Key'],
  ollama: [
    'https://github.com/ollama/ollama/blob/main/docs/api.md',
    'Local — no key needed (default: localhost:11434)',
  ],
  langchain: ['https://docs.langchain.com/', 'LangSmith → Settings → API Keys → Create'],
  'vertex-ai': [
    'https://cloud.google.com/vertex-ai/docs/reference/rest',
    'Google Cloud Console → IAM → Service Account → Key',
  ],
  'aws-bedrock': [
    'https://docs.aws.amazon.com/bedrock/latest/APIReference/',
    'IAM Console → Users → Security credentials → Access keys',
  ],

  // ── Video & Meetings ──
  'google-meet': [
    'https://developers.google.com/meet/api',
    'Google Cloud Console → Credentials → API Key',
  ],
  loom: ['https://developers.loom.com/', 'Settings → Developer Portal → API Key'],
  'otter-ai': ['https://otter.ai/', 'Account → API → Access Token'],
  fireflies: ['https://docs.fireflies.ai/', 'Integrations → Fireflies API → API Key'],
  descript: ['https://developers.descript.com/', 'Settings → API → Token'],
  riverside: ['https://riverside.fm/', 'Account → Integrations → API'],
  streamyard: ['https://streamyard.com/', 'Account → API → Token'],
  daily: ['https://docs.daily.co/reference/rest-api', 'Dashboard → Developers → API Key'],
  jitsi: ['https://jitsi.github.io/handbook/', 'Self-hosted — configure in jitsi config'],
  'twilio-video': ['https://www.twilio.com/docs/video', 'Console → Account → API Keys → Create'],

  // ── Content & CMS ──
  sanity: [
    'https://www.sanity.io/docs/http-api',
    'manage.sanity.io → API → Tokens → Add API Token',
  ],
  prismic: ['https://prismic.io/docs/api', 'Settings → API & Security → Generate Access Token'],
  datocms: [
    'https://www.datocms.com/docs/content-delivery-api',
    'Settings → API tokens → Add new API token',
  ],
  directus: ['https://docs.directus.io/reference/', 'Settings → Access Tokens → Create Token'],
  hashnode: ['https://apidocs.hashnode.com/', 'Settings → Developer → Access Tokens → Generate'],
  substack: ['https://substack.com/', 'Settings → API (limited)'],
  beehiiv: ['https://developers.beehiiv.com/', 'Settings → Integrations → API → API Key'],
  buttondown: ['https://api.buttondown.email/', 'Settings → API → API Key'],
  'notion-cms': [
    'https://developers.notion.com/',
    'Settings → My integrations → Create → Copy token',
  ],
  'hubspot-cms': [
    'https://developers.hubspot.com/docs/api/cms/',
    'Settings → Integrations → Private apps → Create → Token',
  ],
  'devto-content': [
    'https://developers.forem.com/api',
    'Settings → Extensions → DEV Community API Keys → Generate',
  ],
  'payload-cms': ['https://payloadcms.com/docs/rest-api/overview', 'Admin → API Keys → Generate'],
  keystonejs: ['https://keystonejs.com/docs/apis/', 'Self-hosted — configure in keystone config'],

  // ── Automation & Utilities ──
  calcom: ['https://cal.com/docs/api-reference/', 'Settings → Developer → API Keys → Create'],
  zapier: ['https://platform.zapier.com/reference/', 'Developer Platform → API Key'],
  make: ['https://www.make.com/en/api-documentation', 'Profile → API → Create a Token'],
  ifttt: ['https://ifttt.com/docs', 'Developer Dashboard → Service → API Key'],
  'http-request': ['', 'Configure the endpoint URL and any required authentication'],

  // ── Notes & Knowledge ──
  obsidian: ['https://obsidian.md/', 'Local vault — no API key needed'],
  roam: [
    'https://roamresearch.com/#/app/developer-documentation',
    'Settings → API tokens → Generate',
  ],
  craft: ['https://developer.craft.do/', 'Account → Extension → API Token'],

  // ── Finance ──
  'zoho-books': ['https://www.zoho.com/books/api/v3/', 'Developer Console → API Credentials'],
  sage: ['https://developer.sage.com/', 'Developer Portal → Apps → Client Credentials'],

  // ── IoT ──
  tuya: ['https://developer.tuya.com/', 'IoT Platform → Cloud → API Explorer → Access ID/Secret'],
  particle: [
    'https://docs.particle.io/reference/cloud-apis/api/',
    'Console → Authentication → Access Token',
  ],

  // ── Cloud Images ──
  'google-photos': ['https://developers.google.com/photos', 'Google Cloud Console → Credentials'],
  cloudinary: [
    'https://cloudinary.com/documentation/admin_api',
    'Settings → Access Keys → API Key & Secret',
  ],
  imgix: ['https://docs.imgix.com/apis/management', 'Dashboard → API Keys → Generate'],

  // ── Misc utilities with no key ──
  csv: ['', ''],
  pdf: ['', ''],
  ocr: ['', ''],
  qrcode: ['', ''],
  'email-parser': ['', ''],
  'ip-geo': ['', ''],
  'url-shortener': ['', ''],
  regex: ['', ''],
  'json-parse': ['', ''],
  base64: ['', ''],
  'rss-write': ['', ''],
  puppeteer: ['', ''],
  playwright: ['', ''],
  cheerio: ['', ''],
};

export default D;
