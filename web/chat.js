// Chat lines. Everything goes in with textContent, never innerHTML, and each message is its own
// block with dir="auto" so Hebrew and English each lay out in their own direction.

const MAX_LINES = 200;

export class ChatView {
  constructor(logEl) {
    this.log = logEl;
  }

  /** @param {{name?: string, text: string, mine?: boolean, system?: boolean}} line */
  add({ name = '', text, mine = false, system = false }) {
    const li = document.createElement('li');
    li.className = system ? 'chat-system' : mine ? 'chat-mine' : 'chat-theirs';
    if (!system) {
      const who = document.createElement('span');
      who.className = 'chat-name';
      who.dir = 'auto';
      who.textContent = name;
      li.append(who);
    }
    const body = document.createElement('span');
    body.className = 'chat-text';
    body.dir = 'auto';
    body.textContent = text;
    li.append(body);
    this.log.append(li);
    while (this.log.children.length > MAX_LINES) this.log.firstChild.remove();
    this.log.scrollTop = this.log.scrollHeight;
  }
}
