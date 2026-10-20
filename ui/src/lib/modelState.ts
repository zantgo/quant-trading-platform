import type { CrossTfStance, MarketBias } from '../types';

/**
 * The wire calls the cross-duration direction `mtf_stance`; the display
 * vocabulary calls the same reading a bias. One mapping, so no surface invents
 * its own tokens now that the derived `AnalysisMatrix` is gone.
 */
export function stanceToBias(
    stance: CrossTfStance | null | undefined,
): MarketBias {
    switch (stance) {
        case 'strong_bull':
            return 'StrongBullish';
        case 'bull':
            return 'Bullish';
        case 'bear':
            return 'Bearish';
        case 'strong_bear':
            return 'StrongBearish';
        case 'neutral':
            return 'Neutral';
        default:
            return 'Neutral';
    }
}
