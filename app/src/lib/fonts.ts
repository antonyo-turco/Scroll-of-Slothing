// Registers the bundled Liberation Sans (the PDF font) for exact card previews.
import regular from '../../../assets/fonts/LiberationSans-Regular.ttf?url';
import bold from '../../../assets/fonts/LiberationSans-Bold.ttf?url';
import italic from '../../../assets/fonts/LiberationSans-Italic.ttf?url';
import boldItalic from '../../../assets/fonts/LiberationSans-BoldItalic.ttf?url';

export const CARD_FONT = 'Sleuth Liberation Sans';

let done = false;

export function registerCardFonts(): void {
  if (done || typeof document === 'undefined') return;
  done = true;
  const faces: [string, string, string][] = [
    [regular, '400', 'normal'],
    [bold, '700', 'normal'],
    [italic, '400', 'italic'],
    [boldItalic, '700', 'italic'],
  ];
  const css = faces
    .map(([src, weight, style]) => `@font-face{font-family:'${CARD_FONT}';src:url('${src}') format('truetype');font-weight:${weight};font-style:${style};font-display:block}`)
    .join('\n');
  const el = document.createElement('style');
  el.textContent = css;
  document.head.appendChild(el);
}
