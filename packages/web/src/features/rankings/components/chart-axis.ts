interface BuildAxisTicksOptions {
  maxTicks?: number;
  integerOnly?: boolean;
}

export interface AxisScale {
  domainMax: number;
  ticks: number[];
}

export function buildAxisScale(
  maxValue: number,
  { maxTicks = 5, integerOnly = false }: BuildAxisTicksOptions = {},
): AxisScale {
  const safeMax = Number.isFinite(maxValue) && maxValue > 0 ? maxValue : 0;
  const roughStep = safeMax / Math.max(maxTicks - 1, 1);
  const step = getNiceStep(roughStep, integerOnly);
  const domainMax = Math.max(step, Math.ceil(safeMax / step) * step);
  const ticks: number[] = [];

  for (let value = 0; value <= domainMax + step * 0.5; value += step) {
    const normalized = integerOnly
      ? Math.round(value)
      : Number(value.toFixed(6));
    ticks.push(normalized);
  }

  return { domainMax, ticks };
}

function getNiceStep(roughStep: number, integerOnly: boolean): number {
  if (!Number.isFinite(roughStep) || roughStep <= 0) {
    return 1;
  }

  const exponent = Math.floor(Math.log10(roughStep));
  const magnitude = 10 ** exponent;
  const normalized = roughStep / magnitude;

  let niceNormalized: number;
  if (normalized <= 1) {
    niceNormalized = 1;
  } else if (normalized <= 2) {
    niceNormalized = 2;
  } else if (normalized <= 5) {
    niceNormalized = 5;
  } else {
    niceNormalized = 10;
  }

  const step = niceNormalized * magnitude;
  if (!integerOnly) {
    return step;
  }

  return Math.max(1, Math.ceil(step));
}
