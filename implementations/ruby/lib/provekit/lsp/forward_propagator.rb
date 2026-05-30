# ForwardPropagator: accumulate posts and emit implication-check diagnostics.
# Per: docs/lsp/forward-propagation-floor-v1.md
class ForwardPropagator
  def initialize
    @seed_catalog = {}
  end

  def add_to_catalog(callee_id, pre, post)
    @seed_catalog[callee_id] = post
  end

  def check_callsite(callee_id, current_post)
    return nil if current_post[:is_top]
    callee_pre = @seed_catalog[callee_id]
    return nil unless callee_pre

    current_post[:constraints].each do |c|
      unless callee_pre[:constraints].include?(c)
        return { code: "provekit.lsp.implication_failed", message: "post does not imply callee pre: #{callee_pre[:constraints].join(' && ')}" }
      end
    end
    nil
  end
end
