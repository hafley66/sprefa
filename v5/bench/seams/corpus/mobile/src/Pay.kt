package com.org.mobile

// Kotlin mobile client. Third declaration of the cross-language `PaymentClient`.
class PaymentClient {
    fun charge(amount: Long): Boolean {
        return amount > 0
    }
}
