defmodule Ferricstore.PolicyRuntimeCapture.Budget do
  @moduledoc false

  # Reject until the separately reviewed resource kernel and metered caller
  # protocol exist. Capture does not use this interface in the scaffold stage.
  def start(_ready_state), do: {:error, :unsupported_budget_protocol}
  def request(_session, _parent), do: {:error, :unsupported_budget_protocol}
  def permission(_ticket), do: {:error, :unsupported_budget_protocol}
  def issue(_ticket), do: {:error, :unsupported_budget_protocol}
  def cancel(_ticket), do: {:error, :unsupported_budget_protocol}
  def abandon(_ticket), do: {:error, :unsupported_budget_protocol}
  def settle(_ticket, _metered_outcome), do: {:error, :unsupported_budget_protocol}
  def finish(_ticket, _prepared), do: {:error, :unsupported_budget_protocol}
end
