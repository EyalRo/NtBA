// The player list. Names come from other players, so they are only ever set with textContent.

const LINK_TEXT = {
  self: 'you',
  open: 'connected',
  connecting: 'reaching for chat…',
  unreachable: "can't reach for chat or challenges",
};

/**
 * @param {HTMLElement} list a <ul>
 * @param {{id: string, name: string, status: string, link: string, muted: boolean}[]} players
 * @param {{onMute?: (id: string, on: boolean) => void, onChallenge?: (id: string) => void, busy?: () => boolean}} actions
 */
export function renderRoster(list, players, actions = {}) {
  const items = players.map((p) => {
    const li = document.createElement('li');
    li.className = `player player-${p.link}`;
    const name = document.createElement('span');
    name.className = 'player-name';
    name.dir = 'auto';
    name.textContent = p.name;
    const info = document.createElement('span');
    info.className = 'player-info';
    info.textContent = [p.status === 'in-match' ? 'in a match' : '', LINK_TEXT[p.link] ?? ''].filter(Boolean).join(' · ');
    li.append(name, info);
    if (p.link !== 'self') {
      const mute = document.createElement('button');
      mute.type = 'button';
      mute.textContent = p.muted ? 'Unmute' : 'Mute';
      mute.addEventListener('click', () => actions.onMute?.(p.id, !p.muted));
      li.append(mute);
      if (actions.onChallenge) {
        const challenge = document.createElement('button');
        challenge.type = 'button';
        challenge.textContent = 'Challenge';
        challenge.disabled = p.link !== 'open' || p.status !== 'lobby' || Boolean(actions.busy?.());
        challenge.addEventListener('click', () => actions.onChallenge(p.id));
        li.prepend(challenge); // first, so it is the obvious action
      }
    }
    return li;
  });
  list.replaceChildren(...items);
}
