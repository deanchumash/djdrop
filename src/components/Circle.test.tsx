import { render, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Circle } from './Circle';

describe('Circle', () => {
  it('calls onDrop with uri-list data', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.dragOver(el, { dataTransfer: { getData: () => 'https://soundcloud.com/test' } });
    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/uri-list' ? 'https://soundcloud.com/test' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('https://soundcloud.com/test');
  });

  it('falls back to text/plain if no uri-list', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/plain' ? 'Kendrick Lamar HUMBLE' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('Kendrick Lamar HUMBLE');
  });

  it('shows progress ring when progress prop is set', () => {
    const { container } = render(<Circle onDrop={() => {}} progress={50} />);
    expect(container.querySelector('svg')).toBeTruthy();
  });
});
