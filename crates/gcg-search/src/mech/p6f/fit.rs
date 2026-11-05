//! The deterministic fit of the calibrated policy (W-23, PLAN §12.3 S9): a conditional logit
//! over each decision's options, `P(i) = exp(η_i) / Σ_j exp(η_j)` with `η_i = offset_i + β · x_i`,
//! fitted by maximum likelihood with Newton's method (IRLS) in f64.
//!
//! Every sum runs in a fixed order (decisions in record order, options in option order,
//! features in index order), `exp` and `ln` are the portable `libm` functions, and the linear
//! system is solved by a fixed Cholesky elimination, so the fit gives the same bits on every
//! platform and for any thread count of the self-play that produced the records.

use gcg_core::portable;

/// One recorded decision: per option, its fixed log-odds offset and its features; the index
/// of the option the search chose.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Decision {
    pub(crate) offset: Vec<f64>,
    pub(crate) x: Vec<Vec<f64>>,
    pub(crate) chosen: usize,
}

/// A fitted model.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Fit {
    pub(crate) beta: Vec<f64>,
    /// The standard errors from the inverse of the final Hessian (ridge included).
    pub(crate) se: Vec<f64>,
    pub(crate) log_likelihood: f64,
    pub(crate) iterations: u32,
    pub(crate) converged: bool,
}

/// The ridge added to the Hessian's diagonal (and its gradient): it keeps a feature that no
/// decision varies identified at 0.
pub(crate) const RIDGE: f64 = 1e-3;
/// Newton steps at most.
pub(crate) const MAX_ITERATIONS: u32 = 100;
/// Convergence: every coordinate of the step below this.
pub(crate) const TOLERANCE: f64 = 1e-10;

/// The option probabilities of `d` under `beta`, and its log-likelihood term.
fn probabilities(d: &Decision, beta: &[f64]) -> (Vec<f64>, f64) {
    let eta: Vec<f64> = d
        .offset
        .iter()
        .zip(&d.x)
        .map(|(&o, x)| {
            let mut e = o;
            for (b, xi) in beta.iter().zip(x) {
                e += b * xi;
            }
            e
        })
        .collect();
    let top = crate::mech::reference::py_max(&eta);
    let w: Vec<f64> = eta.iter().map(|&e| portable::exp(e - top)).collect();
    let mut total = 0.0;
    for &v in &w {
        total += v;
    }
    let ll = eta[d.chosen] - top - portable::log(total);
    (w.into_iter().map(|v| v / total).collect(), ll)
}

/// The penalised log-likelihood, its gradient and the negated Hessian at `beta`.
fn derivatives(decisions: &[Decision], beta: &[f64]) -> (f64, Vec<f64>, Vec<Vec<f64>>) {
    let k = beta.len();
    let mut ll = 0.0;
    let mut g = vec![0.0; k];
    let mut h = vec![vec![0.0; k]; k];
    for d in decisions {
        let (p, l) = probabilities(d, beta);
        ll += l;
        let mut mean = vec![0.0; k];
        for (pi, x) in p.iter().zip(&d.x) {
            for (m, xi) in mean.iter_mut().zip(x) {
                *m += pi * xi;
            }
        }
        for (gi, (xc, m)) in g.iter_mut().zip(d.x[d.chosen].iter().zip(&mean)) {
            *gi += xc - m;
        }
        for (pi, x) in p.iter().zip(&d.x) {
            for a in 0..k {
                let da = x[a] - mean[a];
                if da == 0.0 {
                    continue;
                }
                for b in 0..k {
                    h[a][b] += pi * da * (x[b] - mean[b]);
                }
            }
        }
    }
    for a in 0..k {
        ll -= 0.5 * RIDGE * beta[a] * beta[a];
        g[a] -= RIDGE * beta[a];
        h[a][a] += RIDGE;
    }
    (ll, g, h)
}

/// The Cholesky factor `L` of a symmetric positive definite `a` (`a = L Lᵀ`), or `None`.
fn cholesky(a: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let k = a.len();
    let mut l = vec![vec![0.0; k]; k];
    for i in 0..k {
        for j in 0..=i {
            let mut s = a[i][j];
            for (a, b) in l[i][..j].iter().zip(&l[j][..j]) {
                s -= a * b;
            }
            if i == j {
                if s <= 0.0 {
                    return None;
                }
                l[i][j] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    Some(l)
}

/// Solves `L Lᵀ x = b`.
fn solve(l: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let k = b.len();
    let mut y = vec![0.0; k];
    for i in 0..k {
        let mut s = b[i];
        for (a, b) in l[i][..i].iter().zip(&y[..i]) {
            s -= a * b;
        }
        y[i] = s / l[i][i];
    }
    let mut x = vec![0.0; k];
    for i in (0..k).rev() {
        let mut s = y[i];
        for m in i + 1..k {
            s -= l[m][i] * x[m];
        }
        x[i] = s / l[i][i];
    }
    x
}

/// Fits `k` coefficients to `decisions` from zero.
///
/// # Panics
/// If a decision's lists disagree in length, an option has other than `k` features, or the
/// chosen index is out of range.
#[must_use]
pub(crate) fn fit(decisions: &[Decision], k: usize) -> Fit {
    for d in decisions {
        assert_eq!(d.offset.len(), d.x.len(), "one offset per option");
        assert!(d.x.iter().all(|x| x.len() == k), "{k} features per option");
        assert!(d.chosen < d.x.len(), "the chosen option is an option");
    }
    let mut beta = vec![0.0; k];
    let mut iterations = 0;
    let mut converged = false;
    while iterations < MAX_ITERATIONS {
        let (_, g, h) = derivatives(decisions, &beta);
        let l = cholesky(&h).expect("the ridge keeps the Hessian positive definite");
        let step = solve(&l, &g);
        iterations += 1;
        for (b, s) in beta.iter_mut().zip(&step) {
            *b += s;
        }
        if step.iter().all(|s| s.abs() < TOLERANCE) {
            converged = true;
            break;
        }
    }
    let (log_likelihood, _, h) = derivatives(decisions, &beta);
    let l = cholesky(&h).expect("the ridge keeps the Hessian positive definite");
    let se = (0..k)
        .map(|i| {
            let mut e = vec![0.0; k];
            e[i] = 1.0;
            solve(&l, &e)[i].sqrt()
        })
        .collect();
    Fit {
        beta,
        se,
        log_likelihood,
        iterations,
        converged,
    }
}

/// The expected share of decisions in which the model (`beta`) picks an option with feature
/// `j` set, and the observed share in which the search did, over `decisions` (W-23's
/// frequency gap).
#[must_use]
pub(crate) fn shares(decisions: &[Decision], beta: &[f64], j: usize) -> (f64, f64) {
    let mut expected = 0.0;
    let mut observed = 0.0;
    let mut n = 0.0;
    for d in decisions {
        if !d.x.iter().any(|x| x[j] != 0.0) {
            continue;
        }
        let (p, _) = probabilities(d, beta);
        for (pi, x) in p.iter().zip(&d.x) {
            if x[j] != 0.0 {
                expected += pi;
            }
        }
        if d.x[d.chosen][j] != 0.0 {
            observed += 1.0;
        }
        n += 1.0;
    }
    if n == 0.0 {
        return (0.0, 0.0);
    }
    (expected / n, observed / n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decisions drawn from a known conditional logit: the fit recovers the coefficients and
    /// is reproducible bit for bit.
    #[test]
    fn irls_recovers_a_known_model_deterministically() {
        let truth = [1.5, -0.75];
        let mut rng = gcg_core::rng::SplitMix64::new(7);
        let mut decisions = Vec::new();
        for _ in 0..4000 {
            let n = 2 + (rng.next_u64() % 3) as usize;
            let x: Vec<Vec<f64>> = (0..n)
                .map(|_| vec![rng.random(), f64::from(u8::from(rng.random() < 0.3))])
                .collect();
            let offset: Vec<f64> = (0..n).map(|_| rng.random() - 0.5).collect();
            let eta: Vec<f64> = (0..n)
                .map(|i| offset[i] + truth[0] * x[i][0] + truth[1] * x[i][1])
                .collect();
            let w: Vec<f64> = eta.iter().map(|&e| portable::exp(e)).collect();
            let total: f64 = w.iter().sum();
            let mut u = rng.random() * total;
            let mut chosen = n - 1;
            for (i, &wi) in w.iter().enumerate() {
                if u < wi {
                    chosen = i;
                    break;
                }
                u -= wi;
            }
            decisions.push(Decision { offset, x, chosen });
        }
        let a = fit(&decisions, 2);
        let b = fit(&decisions, 2);
        assert!(a.converged && a.iterations < 20, "{a:?}");
        assert_eq!(
            a.beta.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            b.beta.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
        );
        for (got, want) in a.beta.iter().zip(truth) {
            assert!((got - want).abs() < 0.25, "{got} vs {want}");
        }
        assert!(a.se.iter().all(|&s| s > 0.0 && s < 0.25), "{:?}", a.se);
        // The fitted model matches the observed shares of the indicator feature (the score
        // equation of a maximum-likelihood fit), up to the ridge.
        let (expected, observed) = shares(&decisions, &a.beta, 1);
        assert!(
            (expected - observed).abs() < 1e-3,
            "{expected} vs {observed}"
        );
    }

    #[test]
    fn cholesky_solves_a_small_system() {
        let a = vec![vec![4.0, 2.0], vec![2.0, 3.0]];
        let l = cholesky(&a).expect("positive definite");
        let x = solve(&l, &[2.0, 1.0]);
        assert!((4.0 * x[0] + 2.0 * x[1] - 2.0).abs() < 1e-12);
        assert!((2.0 * x[0] + 3.0 * x[1] - 1.0).abs() < 1e-12);
        assert!(cholesky(&[vec![0.0]]).is_none());
    }
}
