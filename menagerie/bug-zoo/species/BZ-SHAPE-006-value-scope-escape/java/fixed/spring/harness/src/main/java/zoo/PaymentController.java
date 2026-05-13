package zoo;

import jakarta.validation.constraints.Min;
import org.springframework.web.bind.annotation.RequestParam;

public final class PaymentController {
    public String accept(@RequestParam(defaultValue = "43") @Min(43) int value) {
        return "accepted:" + value;
    }
}
