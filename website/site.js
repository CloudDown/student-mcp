// Le choix reste modifiable, même si le navigateur reconnaît le système.
const ua = navigator.userAgent;
if (!/Android|iPhone|iPad|Mobile/i.test(ua) && !(navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1)) {
  const id = /Windows NT/i.test(ua) ? 'windows' : /Macintosh/i.test(ua) ? 'macos' : /Linux/i.test(ua) && !/CrOS/i.test(ua) ? 'linux' : null;
  if (id) {
    const panel = document.getElementById(id);
    const preferred = document.querySelector(`.os-picker input[value="${id}"]`);
    preferred.checked = true;
    const action = document.getElementById('hero-action');
    const download = panel.querySelector('a.primary');
    if (download) {
      action.href = download.href;
      action.firstElementChild.textContent = download.textContent;
    } else {
      action.firstElementChild.textContent = 'Installer sous Linux';
      action.addEventListener('click', () => { preferred.checked = true; });
    }
  }
}

const topics = document.querySelector('.topics');
const question = document.querySelector('.question');
const answer = document.getElementById('example-answer');
const examples = {
  remise: [question.textContent, answer.textContent],
  consigne: ['Que faut-il inclure dans le compte rendu\u00a0?', 'La consigne demande un tableau de résultats et un paragraphe pour expliquer les écarts observés.'],
  document: ['Quel document décrit le protocole du labo\u00a0?', 'Dans les documents du cours, tu trouveras le protocole dans «\u00a0Laboratoire 2 — Osmose\u00a0».'],
};
topics.addEventListener('change', ({target}) => {
  [question.textContent, answer.textContent] = examples[target.value];
});
topics.hidden = false;

// La vitrine cherche dans un instantané public. Aucune requête au portail.
if (typeof schoolDirectory !== 'undefined') {
  const finder = document.getElementById('finder');
  const input = document.getElementById('school-search');
  const results = document.getElementById('school-results');
  const status = document.getElementById('search-status');
  const platformName = { moodle: 'Moodle', omnivox: 'Omnivox', brightspace: 'Brightspace', canvas: 'Canvas' };
  const normalize = text => text.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase().trim();
  const schools = schoolDirectory.map(([name, portal, aliases]) => ({
    name, portal, key: normalize(name), words: normalize(`${name} ${aliases}`).split(/[\s-]+/)
  }));
  function searchSchools() {
    results.replaceChildren();
    const query = normalize(input.value);
    if (query.length < 2) {
      status.textContent = query ? 'Tape au moins deux caractères.' : 'La recherche utilise l’annuaire de l’app.';
      return;
    }
    const terms = query.split(/[\s-]+/).filter(Boolean);
    const matches = schools.filter(s => terms.length && terms.every(term => s.words.some(word => word.startsWith(term))));
    const rank = school => school.key === query ? 0 : school.key.startsWith(query) ? 1 : school.words.includes(query) ? 2 : 3;
    matches.sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, 'fr'));
    status.textContent = !matches.length ? 'Aucun résultat. Si ton portail utilise une des quatre plateformes, tu peux saisir son adresse dans l’app.' :
      matches.length > 5 ? `5 résultats sur ${matches.length}. Précise ta recherche pour en voir moins.` :
      `${matches.length} établissement${matches.length > 1 ? 's' : ''} dans l’annuaire.`;
    for (const {name, portal} of matches.slice(0, 5)) {
      const item = document.createElement('li');
      const button = document.createElement('button');
      const caption = document.createElement('span');
      button.type = 'button';
      button.append(name, caption);
      caption.textContent = platformName[portal];
      button.addEventListener('click', () => {
        document.querySelector(`input[name="portal"][value="${portal}"]`).checked = true;
        input.value = name;
        results.replaceChildren();
        status.textContent = `${name} utilise ${platformName[portal]} selon l’annuaire.`;
        const heading = document.querySelector(`#portal-${portal} h3`);
        heading.tabIndex = -1;
        heading.focus();
      });
      item.append(button);
      results.append(item);
    }
  }
  input.addEventListener('input', searchSchools);
  finder.hidden = false;
}

document.querySelector('.mode-picker').addEventListener('change', () => {
  document.querySelector('.guide video').pause();
});

if (navigator.clipboard?.writeText) {
  const copy = document.getElementById('copy');
  const status = document.getElementById('copy-status');
  copy.hidden = false;
  copy.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(document.getElementById('command').textContent);
      status.textContent = 'Commande copiée.';
    } catch {
      status.textContent = 'Sélectionne la commande ci-dessus pour la copier.';
    }
  });
}
