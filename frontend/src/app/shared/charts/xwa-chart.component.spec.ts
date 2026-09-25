import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';

import { XwaChartComponent, XwaChartDatum } from './xwa-chart.component';

@Component({
  standalone: true,
  imports: [XwaChartComponent],
  template: `
    <app-xwa-chart [kind]="kind" [data]="data" [unit]="unit" [title]="title" />
  `,
})
class HostComponent {
  kind: XwaChartComponent['kind'] = 'donut';
  data: XwaChartDatum[] = [];
  unit = '';
  title = '';
}

function render(kind: XwaChartComponent['kind'], data: XwaChartDatum[], unit = 'JOBS') {
  TestBed.configureTestingModule({ imports: [HostComponent] });
  const fixture = TestBed.createComponent(HostComponent);
  fixture.componentInstance.kind = kind;
  fixture.componentInstance.data = data;
  fixture.componentInstance.unit = unit;
  fixture.componentInstance.title = 'test chart';
  fixture.detectChanges();
  return fixture;
}

describe('XwaChartComponent', () => {
  it('shows the empty state when every value is zero', () => {
    const fixture = render('donut', [
      { label: 'COMPLETED', value: 0 },
      { label: 'FAILED', value: 0 },
    ]);
    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('svg')).toBeNull();
    expect(element.querySelector('.xwa-chart-empty')?.textContent?.trim()).toBe(
      '[ NO DATA AVAILABLE ]',
    );
  });

  it('renders donut arcs and an HTML legend with data', () => {
    const fixture = render(
      'donut',
      [
        { label: 'COMPLETED', value: 3, color: 'success' },
        { label: 'FAILED', value: 1, color: 'critical' },
      ],
      'JOBS',
    );
    const element: HTMLElement = fixture.nativeElement;
    const svg = element.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg!.getAttribute('viewBox')).toBe('0 0 112 112');
    // Two arcs, one per positive slice.
    expect(svg!.querySelectorAll('path.chart-donut-segment').length).toBe(2);
    // Center total = 4.
    expect(svg!.querySelector('.chart-center-value')?.textContent?.trim()).toBe('4');
    // Legend rows carry label, value and percentage.
    const legend = [...element.querySelectorAll('.legend-row')].map((row) =>
      row.textContent?.trim(),
    );
    expect(legend.length).toBe(2);
    expect(legend[0]).toContain('COMPLETED');
    expect(legend[0]).toContain('3 (75%)');
  });

  it('renders h-bars with one track per positive row', () => {
    const fixture = render('h-bars', [
      { label: 'example.com', value: 12 },
      { label: 'other.com', value: 4 },
    ]);
    const element: HTMLElement = fixture.nativeElement;
    const svg = element.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg!.querySelectorAll('rect.chart-track').length).toBe(2);
    expect(svg!.querySelectorAll('rect.chart-bar-segment').length).toBe(2);
  });

  it('renders a line chart with dots and a polyline', () => {
    const fixture = render('line', [
      { label: '2026-09-20', value: 2 },
      { label: '2026-09-21', value: 5 },
      { label: '2026-09-22', value: 3 },
    ]);
    const element: HTMLElement = fixture.nativeElement;
    const svg = element.querySelector('svg');
    expect(svg).not.toBeNull();
    expect(svg!.querySelector('polyline.chart-line')).not.toBeNull();
    expect(svg!.querySelectorAll('circle.chart-line-dot').length).toBe(3);
  });
});
