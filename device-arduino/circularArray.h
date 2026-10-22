#pragma once

#include <array>

// For iterating through a CircularArray if pushes happen while iterating.
struct CircularArraySequenceCounter final
{
    bool isInitialized = false;
    int seqNr = 0;
    int lastRotations = 0;
};


template<typename T, int maxSize>
class CircularArray final
{
public:
    CircularArray()
    {
        data.fill(T{});
    }

    void push(const T& value)
    {
        data.at(head) = value;
        head = (head + 1) % maxSize;
        if (count < maxSize) {
            ++count;
        } else {
            advanceTail();
        }
    }

    const T& at(int index) const
    {
        return data.at((tail + index) % maxSize);
    }

    int size() const
    {
        return count;
    }

    void clear()
    {
        data.fill(T{});
        head = 0;
        tail = 0;
        count = 0;
        rotations = 0;
    }

    bool full() const
    {
        return count == maxSize;
    }

    bool empty() const
    {
        return count == 0;
    }

    const T& front() const
    {
        return data.at(tail);
    }

    void popFront()
    {
        data.at(tail) = T{}; // release the element's memory (relevant for String)
        advanceTail();
        --count;
    }

    // Access by sequence number for buffered sending.
    //
    // Caveat: We assume that after `n` calls of `iterateBySequence`,
    // we have not more than `n` pushes or `n` pops in the array
    // (consumption is faster than production/deletion).
    const T* iterateBySequence(CircularArraySequenceCounter& seqCounter) const
    {
        // Initialize.
        if (!seqCounter.isInitialized) {
            seqCounter.seqNr = tail;
            seqCounter.lastRotations = rotations;
            seqCounter.isInitialized = true;
        }

        // Decrement sequence number if a rotation happened.
        int rotationsDiff = rotations - seqCounter.lastRotations;
        if (rotationsDiff < 0) rotationsDiff += __INT32_MAX__;
        if (rotationsDiff > 1) {
            return nullptr; // Too many rotations.
        }
        if (rotationsDiff == 1) {
            seqCounter.seqNr -= maxSize;
            if (seqCounter.seqNr < 0) return nullptr; // Old elements were overwritten.
            seqCounter.lastRotations = rotations;
        }

        // Pick element by sequence number.
        if (seqCounter.seqNr < tail + count) {
            const T& value = data.at(seqCounter.seqNr % maxSize);
            ++seqCounter.seqNr;
            return &value;
        } else {
            return nullptr;
        }
    }

    bool isLastSeqNr(const CircularArraySequenceCounter& seqCounter) const
    {
        return seqCounter.isInitialized && seqCounter.seqNr == tail + count;
    }

    // for range-based for loops
    class ConstIterator final
    {
    public:
        explicit ConstIterator(const CircularArray& array, int pos)
            : array(array)
            , pos(pos)
        {}

        const T& operator*() const
        {
            return array.at(pos);
        }
        ConstIterator& operator++()
        {
            ++pos;
            return *this;
        }
        bool operator!=(const ConstIterator& other) const
        {
            return pos != other.pos;
        }

    private:
        const CircularArray& array;
        int pos;
    };

    ConstIterator begin() const
    {
        return ConstIterator(*this, 0);
    }

    ConstIterator end() const
    {
        return ConstIterator(*this, count);
    }

private:
    void advanceTail()
    {
        const int newTail = (tail + 1) % maxSize;
        if (newTail < tail) {
            if (rotations >= __INT32_MAX__) {
                rotations = 0;
            }
            rotations++;
        }
        tail = newTail;
    }

    std::array<T, maxSize> data;
    int head = 0;
    int tail = 0;
    int count = 0;
    int rotations = 0;
};
