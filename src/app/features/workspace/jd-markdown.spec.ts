import { renderJd } from './jd-markdown';

describe('renderJd', () => {
  it('renders markdown structure', () => {
    const html = renderJd('## About\n\n- Build compilers\n- Ship **fast**');
    expect(html).toContain('<h2>About</h2>');
    expect(html).toContain('<li>Build compilers</li>');
    expect(html).toContain('<strong>fast</strong>');
  });

  it('keeps single newlines as line breaks', () => {
    expect(renderJd('Line one\nLine two')).toContain('Line one<br>');
  });
});
